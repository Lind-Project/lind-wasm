//! This module provides a global runtime-state lookup mechanism for lind-3i and lind-wasm, enabling
//! controlled transfers of execution across cages, grates, and threads.
//!
//! In lind-wasm, runtime control is not always confined to a single Wasmtime instance or a single
//! linear call stack. There are two primary scenarios in which lind-3i must explicitly locate and
//! re-enter a different runtime state.
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
//! retrieve the Wasmtime execution context. For `exec` and `exit` calls, it will retrieve the context
//! associated with an arbitrary `cage_id`.
//!
//! ---
//! ## Scenario 2: Grate calls (cross-module execution transfers)
//!
//! The second scenario arises during grate calls. Grate calls involve cross-module execution transfers,
//! where control jumps from one Wasm module to another (for example, from a cage into a grate, or between
//! grates). Supporting these jumps similarly requires the ability to locate and enter the runtime state
//! of a different module than the one currently executing.
//!
//! ---
//! To support both scenarios, lind-3i leverages a key property of lind-wasm’s execution model: each Wasmtime
//! `Store` contains exactly one Wasm `Instance`, and each thread executes within its own independent
//! store / instance pair.
//!
//! ---
//! ## VMContext storage model
//!
//! VMContext pointers are stored globally and indexed by `cage_id`. Two distinct
//! structures are used:
//!
//! 1. **Per-cage VMContext queues (`VMCTX_QUEUES`)**
//!    - Represent the default execution pool for a cage or grate.
//!    - Conceptually correspond to the *main thread* (`tid == 1`).
//!    - Used by normal execution paths, including grate calls.
//!
//! At module creation time, lind-3i extracts the `VMContext` pointer associated with the newly created instance.
//! This `VMContext` uniquely identifies the execution state of that `Store` / `Instance`. The pointer is
//! stored in a global table indexed by `cage_id`. When lind-3i needs to transfer execution to
//! another cage or grate, it looks up the corresponding `VMContext` pointer using the target `cage_id`.
//! Using Wasmtime’s internal mechanisms, the `VMContext` pointer can be used to recover the associated
//! `Store` and `Instance`, allowing execution to resume in the correct runtime context.
//!
//! 2. **Per-cage thread VMContext tables (`VMCTX_THREADS`)**
//!    - Used *only* for non-main threads (`tid != 1`).
//!    - Applicable exclusively to pthread-related syscalls and thread `exit`.
//!    - Each `(cage_id, tid)` maps to at most one `VMContext`.
//!
//! Grate calls never consult the thread table and always acquire execution
//! contexts from the main per-cage queue.
//!
//! The tables intentionally store raw `VMContext` pointers rather than typed store or instance handles.
//! This design avoids Rust lifetime constraints that would otherwise prevent cross-store and cross-instance
//! execution transfers. Correctness instead relies on higher-level invariants enforced by lind-wasm, including
//! the guarantee that `VMContext` pointers remain valid for the lifetime of their associated threads.
//!
//! For each pool, a single instance performs full initialization, including lind-specific memory setup,
//! while additional instances attach to the same linear memory. This design allows a grate to process multiple
//! concurrent requests to the same Wasm linear memory without duplicating address space state.
//!
//! ---
//! ## Concurrency note
//!
//! This module provides *execution routing*, not synchronization. Multiple
//! VMContext instances may share the same linear memory. Grate developers are
//! responsible for ensuring proper synchronization when mutating shared state.
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};
use sysdefs::constants::lind_platform_const;

/// The [`VMContext`](wasmtime_runtime::VMContext) pointer originates from Wasmtime internals and
/// represents the execution state of a Wasm instance. It includes the instance’s memories, tables,
/// globals, and other execution state needed when entering or re-entering Wasm code. Because `VMContext`
/// is an opaque runtime structure, it is stored as a raw pointer (`*mut c_void`) wrapped in a safer
/// abstraction.  For more details, see the documentation in `wasmtime_runtime::vmcontext`.
///
/// `VmCtxWrapper` is a lightweight wrapper around a non-null raw pointer to a `VMContext`.
/// It uses `NonNull<c_void>` to express the invariant that the pointer must never be null.
/// The wrapper is `Copy` and `Clone` so it can be cheaply passed around without ownership transfer.
#[derive(Clone, Copy)]
pub struct VmCtxWrapper {
    pub vmctx: NonNull<c_void>,
}

/// The `VmCtxWrapper` is assumed to be valid for concurrent access according to lind-wasm’s execution
/// model.
unsafe impl Send for VmCtxWrapper {}
unsafe impl Sync for VmCtxWrapper {}

impl VmCtxWrapper {
    // exposes the raw mutable pointer
    #[inline]
    pub fn as_ptr(self) -> *mut c_void {
        self.vmctx.as_ptr()
    }
}

/// Global per-cage `VMContext` execution pools.
///
/// Each cage owns a dedicated FIFO queue of `VMContext` entries. This queue represents the default
/// execution pool and is conceptually associated with the main thread (`tid == 1`).
///
/// Normal execution paths and all grate calls acquire `VMContexts` exclusively
/// from this pool.
static VMCTX_QUEUES: OnceLock<Vec<Mutex<VecDeque<VmCtxWrapper>>>> = OnceLock::new();

/// Per-cage thread-specific `VMContext` table.
///
/// This table is used *only* for non-main threads (`tid != 1`) and exists to support pthread-related
/// syscalls and thread `exit`.
///
/// Each `(cage_id, tid)` maps to at most one `VMContext`. No pooling is performed.
/// This table is not consulted for normal execution or grate calls.
static VMCTX_THREADS: OnceLock<Vec<Mutex<HashMap<u64, VmCtxWrapper>>>> = OnceLock::new();

/// Initialize the global `VMContext` pool.
///
/// This function must be called exactly once during lind-wasm startup, before any `VMContext` is
/// pushed to or retrieved from the pool. It eagerly allocates one empty queue per possible `cage_id`.
pub fn init_vmctx_pool() {
    VMCTX_QUEUES.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(VecDeque::new()))
            .collect()
    });

    VMCTX_THREADS.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(HashMap::new()))
            .collect()
    });
}

/// `get_vmctx`
///
/// Retrieve a VMContext from the specified cage.
///
/// This performs a FIFO pop from the main-thread (`tid == 1`) execution queue.
///
/// # Note on concurrency semantics:
/// Because `VmCtxWrapper` is `Copy`, each caller receives an independent wrapper value. Modifying
/// the returned wrapper itself (for example, reassigning the pointer) does not affect other copies
/// associated with the same `(cage_id, thread_id)` entry. However, this copy semantics applies only
/// to the wrapper, not to the underlying execution state. All copies still reference the same underlying
/// `VMContext` and the same cage / grate memory regions.
///
/// As a result, concurrent requests that mutate shared cage or grate memory can still introduce data races,
/// even though each request holds its own `VMContext` copy.
///
/// For example, if a grate defines shared mutable state such as: `UID_CONST = 10;` and exposes a function:
///
/// ```C
/// update_by_one() {
///    UID_CONST++;
/// }
/// ```
///
/// then multiple concurrent invocations of `update_by_one()` will race on `UID_CONST`, despite each invocation
/// operating through a separate `VmCtxWrapper` copy.
///
/// At present, lind-wasm does not enforce synchronization at this level. Grate developers are responsible for
/// ensuring proper concurrency control whenever their grate code mutates shared memory, for example by using
/// explicit locking, atomic operations, or other synchronization mechanisms appropriate to their execution model.
pub fn get_vmctx(cage_id: u64) -> Option<VmCtxWrapper> {
    let queues = VMCTX_QUEUES.get().expect("VMCTX_QUEUES not initialized");
    let q = queues.get(cage_id as usize).expect("invalid cage_id");
    q.lock().unwrap().pop_front()
}

/// `set_vmctx`
///
/// Inserts a `VMContext` into the global per-cage execution pool. Each call registers one executable Wasmtime
/// instance associated with the given cageid.
///
/// In lind-wasm, `VMContext` are preallocated in pools to enable concurrent request handling within a single
/// cage or grate. At instance execution startup, a fixed number of Wasmtime instances (currently 10) are created
/// for each cage. One instance is fully initialized using `instantiate_with_lind`, which performs lind-specific
/// memory setup. The remaining instances are created using `instantiate_with_lind_thread`, and attach to the same
/// linear memory as the primary instance, serving as backup execution contexts. This is typically called when the
/// `VMContext` becomes available.
///
/// All instances created during this initialization phase are registered through `set_vmctx` and pushed into the
/// global `VMContext` table. At runtime, execution paths acquire an available context from this pool.
///
/// After a grate call finishes execution, the `VMContext` used for that execution is returned to the same pool
/// and made available for subsequent requests.
///
/// The implementation of instance creation and pool population is handled externally, primarily in `run.rs` and
/// the multi-process initialization logic under `lind-multi-process`.
pub fn set_vmctx(cage_id: u64, vmctx: VmCtxWrapper) {
    // Insert the `VMContext` entry in the global table
    let queues = VMCTX_QUEUES.get().expect("VMCTX_QUEUES not initialized");
    let q = queues.get(cage_id as usize).expect("invalid cage_id");
    q.lock().unwrap().push_back(vmctx);
}

/// `rm_vmctx`
///
/// Removes the `VMContext` entry for a given cage and thread.
///
/// It returns the removed `VmCtxWrapper` if one was present.
/// This is typically called when a thread exits or its execution context is torn down.
pub fn rm_vmctx(cage_id: u64) -> bool {
    // Get the global `VMContext` pooling table
    let Some(queues) = VMCTX_QUEUES.get() else {
        // Return false if not initialized
        return false;
    };

    let idx = cage_id as usize;

    // Get the queue for the given cage_id
    let Some(q) = queues.get(idx) else {
        // Return false if invalid cage_id or no queue
        return false;
    };

    // Clear the queue for the given cage_id
    let mut guard = q.lock().unwrap();
    guard.clear();
    true
}

/// Register a VMContext for a non-main thread (`tid != 1`).
///
/// This is used exclusively for pthread-related syscalls and thread exit.
/// Grate calls and normal execution never consult this table.
pub fn set_vmctx_thread(cage_id: u64, tid: u64, vmctx: VmCtxWrapper) {
    debug_assert!(tid != 1, "use set_vmctx_tid1 for tid==1");

    let tables = VMCTX_THREADS.get().expect("VMCTX_THREADS not initialized");
    let t = tables.get(cage_id as usize).expect("invalid cage_id");
    t.lock().unwrap().insert(tid, vmctx);
}

/// Look up the VMContext for a non-main thread (`tid != 1`).
///
/// Returns `None` if the thread has exited or was never registered.
pub fn get_vmctx_thread(cage_id: u64, tid: u64) -> Option<VmCtxWrapper> {
    debug_assert!(tid != 1, "use get_vmctx_tid1 for tid==1");

    let tables = VMCTX_THREADS.get().expect("VMCTX_THREADS not initialized");
    let t = tables.get(cage_id as usize).expect("invalid cage_id");
    t.lock().unwrap().get(&tid).copied()
}

/// tid != 1: remove a single thread entry
pub fn rm_vmctx_thread(cage_id: u64, tid: u64) -> bool {
    debug_assert!(tid != 1, "tid==1 should clear pool differently if needed");

    let Some(tables) = VMCTX_THREADS.get() else {
        return false;
    };
    let Some(t) = tables.get(cage_id as usize) else {
        return false;
    };
    t.lock().unwrap().remove(&tid).is_some()
}
