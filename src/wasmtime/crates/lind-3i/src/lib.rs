//! This module provides a global runtime-state lookup mechanism for lind-3i and lind-wasm, enabling
//! controlled transfers of execution across cages, grates, and threads.
//!
//! In lind-wasm, runtime control is not always confined to a single Wasmtime instance or a single
//! linear call stack. There are two primary scenarios in which lind-3i must explicitly locate and
//! re-enter a different runtime state.
//!
//! Importantly, not all re-entries into Wasmtime are equivalent. Some operations require resuming
//! execution in the *same continuation context* (i.e., the same instance and asyncify state),
//! while others only require access to a compatible instance that shares the same linear memory.
//!
//! The mechanisms in this module distinguish between these cases explicitly.
//!
//! ---
//! ## Scenario 1: Process-like operations (`fork`, `exec`, `exit`)
//!
//! The first scenario occurs during process-like operations such as `fork`, `exec`, and `exit`. These
//! operations require Wasmtime to create, clone, or destroy existing Wasm instances. After RawPOSIX
//! completes the semantic handling of a `fork`, `exec`, or `exit` operation, execution must return to
//! Wasmtime to continue running Wasm code. Importantly, the cage that performs the `fork`, `exec`, or
//! `exit` logic is not necessarily the same cage or grate that originally issued the system call. As
//! a result, lind-3i cannot rely on an implicit “current” runtime state. Instead, it must be able to
//! retrieve the Wasmtime execution context.
//!
//! These operations are also continuation-sensitive, which means that the execution must resume in
//! the *same Wasmtime instance* that originally issued the syscall.
//!
//! In particular, `fork` / `exit` rely on Asyncify to suspend and later resume execution via paired
//! `start_unwind` / `stop_unwind` and `start_rewind` / `stop_rewind` operations. These transitions
//! must occur within the same continuation context, which is the same Wasmtime instance and
//! associated asyncify runtime state. Resuming execution in a different instance, even one that
//! shares linear memory, breaks this invariant and can result in missed callbacks or incorrect
//! return values.
//!
//! As a result, lind-3i must be able to retrieve *the active execution context* corresponding
//! to a specific `(cage_id, tid)` when handling these operations.
//!
//! ---
//! ## Scenario 2: Grate calls (cross-module execution transfers)
//!
//! The second scenario arises during grate calls. Grate calls involve cross-module execution transfers,
//! where control jumps from one Wasm module to another (for example, from a cage into a grate, or between
//! grates). Supporting these jumps similarly requires the ability to locate and enter the runtime state
//! of a different module than the one currently executing.
//!
//! Unlike fork, exec, and exit, grate calls are not continuation-sensitive. A grate call does not need
//! to resume execution in the exact Wasmtime instance that originally issued the transfer. Instead, it
//! only needs to enter a compatible execution context for the target grate. In lind-3i, this compatible
//! context is represented not as a single shared runtime state, but as a pool of independent grate workers.
//! Each worker consists of:
//! - its own Wasmtime `Store`,
//! - its own instantiated grate `Instance`, and
//! - its own independent Wasm call stack region.
//!
//! Operationally, a grate call is executed by leasing one worker from the target grate’s worker pool,
//! invoking the grate entry function inside that worker, and returning the worker to the pool when the
//! call finishes. This means that a grate call should be understood as a transfer into an available worker
//! context, rather than as a re-entry into one globally shared grate instance. This worker-based
//! structure is what makes grate-call concurrency possible.
//!
//! A Wasmtime `Store` is an execution boundary: it owns the runtime state associated with a particular
//! instance execution, including stack state and other mutable execution-local state. By giving each
//! worker its own `Store` and `Instance`, lind-3i ensures that concurrent grate calls do not execute
//! inside the same Wasmtime runtime context. As a result, parallel grate calls do not contend on a shared
//! Wasm call stack, do not overwrite each other’s instance-local execution state, and do not require
//! continuation matching of the kind needed by Asyncify-based process operations.
//!
//! This design also explains why grate calls use a different lookup mechanism from continuation-sensitive
//! operations. For fork / exit, lind-3i must recover the specific active execution context associated with
//! a given (cage_id, tid), because execution must resume in the same continuation. For grate calls, by
//! contrast, lind-3i only needs to obtain some available worker for the target grate, because correctness
//! depends on entering a compatible grate instance, not on resuming a previously suspended continuation.
use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use sysdefs::constants::lind_platform_const;
use sysdefs::constants::lind_platform_const::*;
use wasmtime::{Engine, Instance, Linker, Module, Store, TypedFunc, Val};

type PassFptrTyped = TypedFunc<
    (
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ),
    i32,
>;

type WorkerId = u64;

/// Concurrency policy for a grate handler.
///
/// This determines whether multiple submitted grate calls may execute
/// concurrently on different workers, or whether entry must be serialized.
pub enum ConcurrencyMode {
    /// Allow multiple calls to execute concurrently as long as distinct
    /// workers are available in the pool.
    Parallel,

    /// Allow multiple calls to execute concurrently as long as distinct
    /// workers are available in the pool.
    Serialized,
}

/// Template used to instantiate grate workers.
///
/// A `GrateTemplate` contains the shared, immutable ingredients needed to
/// construct worker-local execution contexts for the same grate module.
/// Each worker clones or reuses these components to create its own
/// `Store + Instance` runtime state.
pub struct GrateTemplate<T> {
    /// The Wasmtime engine used to create worker-local stores and instances.
    ///
    /// This is shared across all workers for the same grate.
    pub engine: Engine,

    /// The compiled grate module that each worker instantiates.
    ///
    /// All workers created from the same template execute this same module,
    /// but do so inside independent stores / instances.
    pub module: Module,

    /// The linker used to instantiate the grate module and attach its imports.
    ///
    /// Each worker starts from this template linker and clones it during
    /// worker creation so that instantiation can proceed independently.
    pub linker: Linker<T>,
}

/// Marshalled arguments for one grate call.
///
/// A `GrateRequest` represents one cross-module execution transfer into a
/// grate worker. It includes the callee function pointer together with the
/// calling cage identity and up to six `(value, cageid)` argument pairs.
///
/// The `argNcageid` fields identify the ownership / address-space context
/// associated with pointer-like arguments, allowing the callee side to
/// interpret cross-cage values correctly.
pub struct GrateRequest {
    /// Address of the function ptr argument passed by the caller,
    /// used for indirect dispatch inside the grate.
    pub handler_addr: u64,
    /// Identity of the calling cage, used for invoking the correct handler.
    pub cageid: u64,
    pub arg1: u64,
    pub arg1cageid: u64,
    pub arg2: u64,
    pub arg2cageid: u64,
    pub arg3: u64,
    pub arg3cageid: u64,
    pub arg4: u64,
    pub arg4cageid: u64,
    pub arg5: u64,
    pub arg5cageid: u64,
    pub arg6: u64,
    pub arg6cageid: u64,
}

struct SerialExecutor {
    lock: Mutex<()>,
}

impl SerialExecutor {
    /// Create a serialization gate for grate calls.
    ///
    /// This executor is used when a grate handler runs in `Serialized` mode.
    /// In that mode, callers may still submit requests concurrently, but only
    /// one request is allowed to enter the grate at a time.
    fn new() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }

    /// Enter the serialized execution region.
    ///
    /// This acquires the internal mutex and returns a guard that keeps the
    /// grate in exclusive-execution mode for the duration of the call.
    /// If the mutex has been poisoned, execution continues by recovering the
    /// inner guard, since poisoning does not invalidate the grate runtime state.
    fn enter(&self) -> MutexGuard<'_, ()> {
        match self.lock.lock() {
            Ok(guard) => {
                #[cfg(feature = "debug-grate-calls")]
                {
                    println!("SerialExecutor: acquired lock");
                }

                guard
            }
            Err(poisoned) => {
                #[cfg(feature = "debug-grate-calls")]
                {
                    println!("SerialExecutor: lock poisoned, but continuing anyway");
                }

                poisoned.into_inner()
            }
        }
    }
}

/// One reusable grate execution worker.
///
/// A `GrateWorker` is the concrete execution unit for grate calls. Each worker
/// owns its own Wasmtime `Store` and `Instance`, but may still be attached to
/// the same underlying linear memory as other workers. To preserve isolation,
/// each worker is assigned a dedicated stack slot inside the shared stack arena.
struct GrateWorker<T> {
    /// Logical identifier of this worker within the handler’s pool.
    ///
    /// The worker id is also used to derive the worker’s private stack slot.
    worker_id: WorkerId,

    /// Worker-local Wasmtime store.
    ///
    /// This store holds the execution state for this worker and isolates its
    /// runtime context from other concurrently executing workers.
    store: Store<T>,

    /// Worker-local instance of the grate module.
    ///
    /// Calls submitted to this worker execute inside this instance.
    instance: Instance,

    /// Typed handle to the grate entry export, if present.
    ///
    /// This is usually the `pass_fptr_to_wt` trampoline used to enter the
    /// grate from the handler. It is cached here to avoid resolving the export
    /// on every call.
    pass_fptr_func: Option<PassFptrTyped>,

    /// Base address of this worker’s assigned stack slot in the stack arena.
    ///
    /// This marks the first usable byte of the worker’s dedicated stack region.
    stack_base: u32,

    /// Top address of this worker’s assigned stack slot.
    ///
    /// Before each call, the worker resets its `__stack_pointer` to this value
    /// so execution starts from a clean stack state within its own slot.
    stack_top: u32,
}

/// Compute the base address of the stack region assigned to a specific worker.
///
/// Each grate worker owns a dedicated stack slot inside the global stack arena.
/// This function maps a logical `worker_id` to the first usable byte of that
/// worker’s stack, skipping over the guard region placed before the slot.
fn worker_stack_base(workerid: WorkerId) -> u32 {
    let stack_arena_base = STACK_ARENA_BASE.get().copied().unwrap_or_else(|| {
        panic!("STACK_ARENA_BASE is not initialized");
    });
    stack_arena_base
        + (workerid as u32 - 1) * (GRATE_STACK_GUARD_SIZE + GRATE_STACK_SLOT_SIZE)
        + GRATE_STACK_GUARD_SIZE
}

/// Compute the top address of the stack region assigned to a specific worker.
///
/// The returned address is used to reset the worker’s `__stack_pointer` before
/// starting a new grate call, ensuring that each invocation begins with a clean
/// stack state inside that worker’s private stack slot.
fn worker_stack_top(workerid: WorkerId) -> u32 {
    worker_stack_base(workerid) + GRATE_STACK_SLOT_SIZE
}

/// Scoped ownership of a borrowed worker.
///
/// A `WorkerLease` represents a worker temporarily checked out from a
/// `GrateHandler`. When the lease is dropped, the worker is automatically
/// returned to the handler’s pool.
struct WorkerLease<'a, T> {
    /// The handler that owns the leased worker.
    ///
    /// This is used to return the worker to the pool on drop.
    owner: &'a GrateHandler<T>,

    /// The leased worker, if still held by this lease.
    ///
    /// The worker is wrapped in `Option` so it can be taken during drop and
    /// returned exactly once.
    worker: Option<GrateWorker<T>>,
}

impl<'a, T> WorkerLease<'a, T> {
    /// Create a scoped lease for a grate worker borrowed from a handler.
    ///
    /// A `WorkerLease` ensures that the worker is automatically returned to the
    /// owning handler when the lease is dropped, even if the grate call exits
    /// early due to an error or trap.
    fn new(owner: &'a GrateHandler<T>, worker: GrateWorker<T>) -> Self {
        Self {
            owner,
            worker: Some(worker),
        }
    }

    /// Get mutable access to the leased worker.
    ///
    /// This is used by the submission path to run a grate request inside the
    /// borrowed worker before the worker is returned to the pool on drop.
    fn worker_mut(&mut self) -> &mut GrateWorker<T> {
        self.worker.as_mut().unwrap()
    }
}

impl<'a, T> Drop for WorkerLease<'a, T> {
    /// Return the leased worker to its handler when the lease goes out of scope.
    ///
    /// This guarantees that worker-pool bookkeeping remains correct even when
    /// execution unwinds due to an error or trap.
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.owner.return_worker(worker);
        }
    }
}

/// Scheduler and worker-pool owner for one grate.
///
/// A `GrateHandler` manages the reusable worker pool for a grate and defines
/// how incoming grate requests are admitted, scheduled, and shut down.
/// It is the main runtime object responsible for grate-call concurrency.
pub struct GrateHandler<T> {
    /// Identifier of the grate or cage associated with this handler.
    ///
    /// This is mainly used for diagnostics and error reporting.
    grate_id: u64,

    /// Id of the designated main worker.
    ///
    /// This field records the canonical first worker in the pool.
    ///
    /// todo:
    /// It may be useful for debugging or future policy decisions, even
    /// though the current submission path leases any available worker.
    main_worker: WorkerId,

    /// Configured concurrency policy for this grate.
    ///
    /// Determines whether `submit()` dispatches through serialized or parallel
    /// execution.
    concurrency_mode: ConcurrencyMode,

    /// Serialization gate used only when the handler is in `Serialized` mode.
    serial_executor: SerialExecutor,

    /// Mutex-protected internal worker-pool state.
    ///
    /// This protects the queue of available workers.
    inner: Mutex<GrateHandlerInner<T>>,

    /// Condition variable used to block until a worker becomes available or
    /// until shutdown-related state changes.
    cv: Condvar,

    /// Flag indicating that shutdown has started.
    ///
    /// Once set, new submissions are rejected, while existing in-flight calls
    /// are allowed to complete
    ///
    /// todo: not integrated with actual grate teardown yet
    shutting_down: AtomicBool,

    /// Number of grate calls currently in flight.
    ///
    /// This is used to coordinate graceful shutdown and to detect when the
    /// handler has become idle.
    ///
    /// todo: not integrated with actual grate teardown yet
    active_calls: AtomicUsize,
}

/// Mutex-protected internal state for a `GrateHandler`.
///
/// This structure exists to keep the lock scope narrow and separate the
/// worker-pool state from the rest of the handler’s control fields.
struct GrateHandlerInner<T> {
    workers: VecDeque<GrateWorker<T>>,
}

impl<T: Clone> GrateHandler<T> {
    /// Pre-create the worker pool for a grate handler.
    ///
    /// Each worker is an independent `store + instance + call stack` execution
    /// context for the same grate module. Pre-initializing the full pool allows
    /// later grate calls to lease a ready-to-run worker without paying the cost
    /// of instantiation on the fast path.
    ///
    /// This worker replication is what enables grate-call concurrency: parallel
    /// calls execute in different Wasmtime stores rather than contending on a
    /// single shared execution context.
    fn init_ten_workers(
        &mut self,
        template: &GrateTemplate<T>,
        host: &T,
        cageid: u64,
    ) -> anyhow::Result<()> {
        for handler_id in 1_u64..=MAX_GRATE_WORKERS as u64 {
            let worker = create_worker(template, host.clone(), handler_id).with_context(|| {
                format!(
                    "failed to create worker {} for cageid {}",
                    handler_id, cageid
                )
            })?;

            self.inner.lock().unwrap().workers.push_back(worker);
        }

        self.main_worker = 1;
        Ok(())
    }
}

impl<T> GrateHandler<T> {
    /// Lease one available worker from the pool, blocking until one is free.
    ///
    /// This function is the core worker-pool acquisition primitive. If all
    /// workers are currently in use, the caller waits on the condition variable
    /// until another call finishes and returns a worker to the pool.
    fn take_worker_blocking(&self) -> GrateWorker<T> {
        let mut inner = self.inner.lock().unwrap();

        loop {
            if let Some(worker) = inner.workers.pop_front() {
                return worker;
            }
            inner = self.cv.wait(inner).unwrap();
        }
    }

    /// Return a worker to the pool and wake one waiting submitter.
    ///
    /// This makes the worker available for reuse by future grate calls.
    /// Returning workers through the handler centralizes pool management and
    /// ensures that blocked callers can resume when capacity becomes available.
    fn return_worker(&self, worker: GrateWorker<T>) {
        let mut inner = self.inner.lock().unwrap();
        inner.workers.push_back(worker);
        self.cv.notify_one();
    }

    /// Mark this grate handler as shutting down.
    ///
    /// After shutdown begins, new submissions are rejected by `ActiveCallGuard`.
    /// Existing in-flight calls are allowed to finish, and waiters are notified
    /// so that shutdown coordination can make progress.
    ///
    /// todo: not integrated with actual grate teardown yet
    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.cv.notify_all();
    }

    /// Block until all in-flight grate calls have completed.
    ///
    /// This is typically used during shutdown after `begin_shutdown()` has
    /// prevented new calls from entering. The function waits until the active
    /// call count drops to zero.
    ///
    /// todo: not integrated with actual grate teardown yet
    pub fn wait_for_idle(&self) {
        let mut guard = self.inner.lock().unwrap();
        while self.active_calls.load(Ordering::Acquire) != 0 {
            guard = self.cv.wait(guard).unwrap();
        }
    }

    /// Execute a grate request under serialized execution.
    ///
    /// This path acquires the serialization lock before leasing a worker,
    /// ensuring that at most one call enters the grate at a time even though
    /// the handler may still own multiple workers.
    fn submit_serialized(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let _serial_guard = self.serial_executor.enter();
        let worker = self.take_worker_blocking();
        let mut lease = WorkerLease::new(self, worker);
        lease.worker_mut().run(req)
    }

    /// Execute a grate request under parallel execution.
    ///
    /// In parallel mode, the handler simply leases an available worker and
    /// runs the request immediately. Different callers may therefore execute
    /// concurrently as long as different workers are available.
    fn submit_parallel(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let worker = self.take_worker_blocking();
        let mut lease = WorkerLease::new(self, worker);
        lease.worker_mut().run(req)
    }

    /// Submit a grate request to this handler.
    ///
    /// This is the main entry point for grate calls. It first registers the
    /// request as an active in-flight call, rejecting it if shutdown has begun,
    /// and then dispatches the request according to the handler’s configured
    /// concurrency mode.
    pub fn submit(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let _active_guard = ActiveCallGuard::new(self)?;

        match self.concurrency_mode {
            ConcurrencyMode::Serialized => self.submit_serialized(req),
            ConcurrencyMode::Parallel => self.submit_parallel(req),
        }
    }
}

/// RAII guard representing one active in-flight grate call.
///
/// An `ActiveCallGuard` increments the handler’s active-call counter when a
/// submission begins and decrements it automatically when execution ends,
/// ensuring correct shutdown coordination even in the presence of errors.
struct ActiveCallGuard<'a, T> {
    owner: &'a GrateHandler<T>,
}

impl<'a, T> ActiveCallGuard<'a, T> {
    /// Register one in-flight grate call against the handler.
    ///
    /// This guard prevents shutdown races by incrementing the active-call count
    /// before execution begins and double-checking whether shutdown started in
    /// the small window after the increment. If shutdown is already in progress,
    /// the increment is rolled back and submission fails.
    fn new(owner: &'a GrateHandler<T>) -> anyhow::Result<Self> {
        if owner.shutting_down.load(Ordering::Acquire) {
            anyhow::bail!("grate handler {} is shutting down", owner.grate_id);
        }

        owner.active_calls.fetch_add(1, Ordering::AcqRel);

        // double-check, avoid shutdown between fetch_add and return
        if owner.shutting_down.load(Ordering::Acquire) {
            owner.active_calls.fetch_sub(1, Ordering::AcqRel);
            owner.cv.notify_all();
            anyhow::bail!("grate handler {} is shutting down", owner.grate_id);
        }

        Ok(Self { owner })
    }
}

impl<'a, T> Drop for ActiveCallGuard<'a, T> {
    /// Deregister one in-flight grate call.
    ///
    /// Dropping this guard decrements the active-call count and notifies waiters,
    /// allowing shutdown code to observe when the handler has become idle.
    fn drop(&mut self) {
        self.owner.active_calls.fetch_sub(1, Ordering::AcqRel);
        self.owner.cv.notify_all();
    }
}

impl<T> GrateWorker<T> {
    /// Reset this worker’s stack pointer to the top of its private stack slot.
    ///
    /// Grate workers are reusable execution contexts. Before each new grate call,
    /// the worker’s Wasm stack pointer is reset so that the next invocation starts
    /// from a clean stack state rather than inheriting frames or stack position
    /// from a previous call.
    fn reset_worker_stack(&mut self) {
        let sp = self.stack_top;
        let stack_global = self
            .instance
            .get_global(&mut self.store, "__stack_pointer")
            .expect("missing __stack_pointer");

        stack_global
            .set(&mut self.store, Val::I32(sp as i32))
            .expect("failed to set __stack_pointer");
    }

    /// Run one grate request inside this worker.
    ///
    /// Execution happens inside this worker’s private `Store` and `Instance`,
    /// which isolates its runtime state from other concurrently executing workers.
    /// The worker resets its stack, resolves the exported grate entry function,
    /// and invokes `pass_fptr_to_wt` with the marshalled request arguments.
    fn run(&mut self, req: GrateRequest) -> anyhow::Result<i32> {
        #[cfg(feature = "debug-grate-calls")]
        {
            println!(
                "Worker {} handling grate request for cage {}, handler_addr: {:#x}",
                self.worker_id, req.cageid, req.handler_addr
            );
        }

        self.reset_worker_stack();

        let func = self.pass_fptr_func.as_ref().ok_or_else(|| {
            anyhow::anyhow!("no pass_fptr_func found in worker {}", self.worker_id)
        })?;

        let ret = func
            .call(
                &mut self.store,
                (
                    req.handler_addr,
                    req.cageid,
                    req.arg1,
                    req.arg1cageid,
                    req.arg2,
                    req.arg2cageid,
                    req.arg3,
                    req.arg3cageid,
                    req.arg4,
                    req.arg4cageid,
                    req.arg5,
                    req.arg5cageid,
                    req.arg6,
                    req.arg6cageid,
                ),
            )
            .map_err(|e| {
                anyhow::anyhow!(
                    "pass_fptr_to_wt trapped in worker {}: {:#}",
                    self.worker_id,
                    e
                )
            })?;

        #[cfg(feature = "debug-grate-calls")]
        println!(
            "Worker {} got result {} from pass_fptr_to_wt",
            self.worker_id, ret
        );
        Ok(ret)
    }
}

/// Create a single grate worker.
///
/// A worker is an independently executable `Store + Instance + call stack`
/// context for one grate module. Workers are the unit of grate-call execution:
/// each submitted request runs inside one worker borrowed from the handler’s
/// pool.
///
/// Although different workers own different Wasmtime `Store`s and `Instance`s,
/// they are attached to the same underlying linear memory region. As a result,
/// workers must not share the same stack range inside linear memory.
///
/// To preserve isolation between concurrent grate calls, lind-wasm partitions
/// the grate stack arena into per-worker stack slots at instantiation time.
/// Each worker is then assigned its own dedicated stack slot, and later resets
/// its `__stack_pointer` to that slot before beginning execution.
///
/// The stack-slot partitioning is established by
/// `instantiate_with_lind_thread()`. See that function’s comments for the
/// detailed layout and attachment semantics.
pub fn create_worker<T>(
    template: &GrateTemplate<T>,
    host: T,
    worker_id: WorkerId,
) -> anyhow::Result<GrateWorker<T>>
where
    T: Clone,
{
    let mut store = Store::new(&template.engine, host);

    let mut linker: Linker<T> = template.linker.clone();

    let stack_arena_base = STACK_ARENA_BASE.get().copied().unwrap_or_else(|| {
        panic!("STACK_ARENA_BASE is not initialized");
    });

    let (instance, _, _) = linker
        .instantiate_with_lind_thread(&mut store, &template.module, false)
        .context("failed to instantiate grate module")?;

    let pass_fptr_func = match instance.get_export(&mut store, "pass_fptr_to_wt") {
        Some(_) => Some(instance.get_typed_func::<(
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
        ), i32>(&mut store, "pass_fptr_to_wt")?),
        None => None,
    };

    let stack_base = worker_stack_base(worker_id);
    let stack_top = worker_stack_top(worker_id);

    Ok(GrateWorker {
        worker_id,
        store,
        instance,
        pass_fptr_func,
        stack_base,
        stack_top,
    })
}

/// Create and initialize a grate handler for one cage.
///
/// A `GrateHandler` owns the reusable worker pool for the target grate and
/// defines how incoming grate calls are scheduled. In `Parallel` mode, multiple
/// calls may execute concurrently by leasing different workers. In `Serialized`
/// mode, calls still use the same worker-pool abstraction, but entry is gated
/// so that only one call runs at a time.
///
/// This function performs eager worker creation so that the handler is ready to
/// serve grate calls immediately after registration.
pub fn create_handler_for_cage<T: Clone>(
    template: &GrateTemplate<T>,
    host: T,
    cageid: u64,
    concurrency_mode: ConcurrencyMode,
) -> anyhow::Result<GrateHandler<T>> {
    let mut handler = GrateHandler {
        grate_id: cageid,
        main_worker: 1,
        concurrency_mode,
        serial_executor: SerialExecutor::new(),
        inner: Mutex::new(GrateHandlerInner {
            workers: VecDeque::new(),
        }),
        cv: Condvar::new(),
        shutting_down: AtomicBool::new(false),
        active_calls: AtomicUsize::new(0),
    };

    handler.init_ten_workers(template, &host, cageid)?;

    Ok(handler)
}

#[derive(Clone, Copy)]
pub struct VmCtxWrapper {
    pub vmctx: NonNull<c_void>,
}

unsafe impl Send for VmCtxWrapper {}
unsafe impl Sync for VmCtxWrapper {}

impl VmCtxWrapper {
    // exposes the raw mutable pointer
    #[inline]
    pub fn as_ptr(self) -> *mut c_void {
        self.vmctx.as_ptr()
    }
}

/// Per-cage, per-thread *active* `VMContext` table.
///
/// This table stores the *currently active* Wasmtime execution context for each thread and is
/// used exclusively for **continuation-sensitive operations** that must resume execution in the
/// same Wasmtime instance that originally issued the syscall.
static VMCTX_THREADS: OnceLock<Vec<Mutex<HashMap<u64, VmCtxWrapper>>>> = OnceLock::new();

/// Initialize the global `VMContext` pool.
///
/// This function must be called exactly once during lind-wasm startup, before any `VMContext` is
/// pushed to or retrieved from the pool. It eagerly allocates one empty queue per possible `cage_id`.
pub fn init_vmctx_pool() {
    VMCTX_THREADS.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(HashMap::new()))
            .collect()
    });
}

/// Register a VMContext according to `(cage_id, tid)` in the per-thread active table.
///
/// This is used exclusively for pthread-related syscalls and thread exit.
/// Grate calls and normal execution never consult this table.
pub fn set_vmctx_thread(cage_id: u64, tid: u64, vmctx: VmCtxWrapper) {
    let tables = VMCTX_THREADS.get().expect("VMCTX_THREADS not initialized");
    let t = tables.get(cage_id as usize).expect("invalid cage_id");
    t.lock().unwrap().insert(tid, vmctx);
}

/// Look up the VMContext
///
/// Returns `None` if the thread has exited or was never registered.
pub fn get_vmctx_thread(cage_id: u64, tid: u64) -> Option<VmCtxWrapper> {
    let tables = VMCTX_THREADS.get().expect("VMCTX_THREADS not initialized");
    let t = tables.get(cage_id as usize).expect("invalid cage_id");
    t.lock().unwrap().get(&tid).copied()
}

/// Remove a single thread entry.
///
/// Special case:
/// - if `tid == 0`, remove all VMContext entries under `cage_id`.
pub fn rm_vmctx_thread(cage_id: u64, tid: u64) -> bool {
    let Some(tables) = VMCTX_THREADS.get() else {
        println!("rm_vmctx_thread: VMCTX_THREADS not initialized");
        return false;
    };
    let Some(t) = tables.get(cage_id as usize) else {
        println!("rm_vmctx_thread: invalid cage_id {}", cage_id);
        return false;
    };

    let mut guard = t.lock().unwrap();

    if tid == 0 {
        let had_entries = !guard.is_empty();
        guard.clear();
        had_entries
    } else {
        guard.remove(&tid).is_some()
    }
}
