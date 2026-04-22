use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use sysdefs::constants::lind_platform_const;
use sysdefs::constants::lind_platform_const::*;
use threei::threei_const;
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

pub struct GrateTemplate<T> {
    pub engine: Engine,
    pub module: Module,
    pub linker: Linker<T>,
}

pub struct GrateRequest {
    pub handler_addr: u64,
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
    fn new() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }

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

struct GrateWorker<T> {
    worker_id: WorkerId,
    store: Store<T>,
    instance: Instance,
    pass_fptr_func: Option<PassFptrTyped>,
    stack_base: u32,
    stack_top: u32,
}

fn worker_stack_base(workerid: WorkerId) -> u32 {
    let stack_arena_base = STACK_ARENA_BASE.get().copied().unwrap_or_else(|| {
        panic!("STACK_ARENA_BASE is not initialized");
    });
    stack_arena_base
        + (workerid as u32 - 1) * (GRATE_STACK_GUARD_SIZE + GRATE_STACK_SLOT_SIZE)
        + GRATE_STACK_GUARD_SIZE
}

fn worker_stack_top(workerid: WorkerId) -> u32 {
    worker_stack_base(workerid) + GRATE_STACK_SLOT_SIZE
}

struct WorkerLease<'a, T> {
    owner: &'a GrateHandler<T>,
    worker: Option<GrateWorker<T>>,
}

impl<'a, T> WorkerLease<'a, T> {
    fn new(owner: &'a GrateHandler<T>, worker: GrateWorker<T>) -> Self {
        Self {
            owner,
            worker: Some(worker),
        }
    }

    fn worker_mut(&mut self) -> &mut GrateWorker<T> {
        self.worker.as_mut().unwrap()
    }
}

impl<'a, T> Drop for WorkerLease<'a, T> {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.owner.return_worker(worker);
        }
    }
}

pub enum ConcurrencyMode {
    Parallel,
    Serialized,
}

pub struct GrateHandler<T> {
    grate_id: u64,
    main_worker: WorkerId,
    concurrency_mode: ConcurrencyMode,
    serial_executor: SerialExecutor,

    inner: Mutex<GrateHandlerInner<T>>,
    cv: Condvar,

    shutting_down: AtomicBool,
    active_calls: AtomicUsize,
}

struct GrateHandlerInner<T> {
    workers: VecDeque<GrateWorker<T>>,
}

impl<T: Clone> GrateHandler<T> {
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
    fn take_worker_blocking(&self) -> GrateWorker<T> {
        let mut inner = self.inner.lock().unwrap();

        loop {
            if let Some(worker) = inner.workers.pop_front() {
                return worker;
            }
            inner = self.cv.wait(inner).unwrap();
        }
    }

    fn return_worker(&self, worker: GrateWorker<T>) {
        let mut inner = self.inner.lock().unwrap();
        inner.workers.push_back(worker);
        self.cv.notify_one();
    }

    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.cv.notify_all();
    }

    pub fn wait_for_idle(&self) {
        let mut guard = self.inner.lock().unwrap();
        while self.active_calls.load(Ordering::Acquire) != 0 {
            guard = self.cv.wait(guard).unwrap();
        }
    }

    fn submit_serialized(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let _serial_guard = self.serial_executor.enter();
        let worker = self.take_worker_blocking();
        let mut lease = WorkerLease::new(self, worker);
        lease.worker_mut().run(req)
    }

    fn submit_parallel(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let worker = self.take_worker_blocking();
        let mut lease = WorkerLease::new(self, worker);
        lease.worker_mut().run(req)
    }

    pub fn submit(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let _active_guard = ActiveCallGuard::new(self)?;

        match self.concurrency_mode {
            ConcurrencyMode::Serialized => self.submit_serialized(req),
            ConcurrencyMode::Parallel => self.submit_parallel(req),
        }
    }
}

struct ActiveCallGuard<'a, T> {
    owner: &'a GrateHandler<T>,
}

impl<'a, T> ActiveCallGuard<'a, T> {
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
    fn drop(&mut self) {
        self.owner.active_calls.fetch_sub(1, Ordering::AcqRel);
        self.owner.cv.notify_all();
    }
}

impl<T> GrateWorker<T> {
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

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::OnceLock;

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

static VMCTX_THREADS: OnceLock<Vec<Mutex<HashMap<u64, VmCtxWrapper>>>> = OnceLock::new();

pub fn init_vmctx_pool() {
    VMCTX_THREADS.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(HashMap::new()))
            .collect()
    });
}

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
