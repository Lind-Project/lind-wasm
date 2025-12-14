//! This module provides a global runtime-state lookup mechanism for lind-3i and lind-wasm, enabling
//! controlled transfers of execution across cages, grates, and threads.
//!
//! In lind-wasm, runtime control is not always confined to a single Wasmtime instance or a single
//! linear call stack. There are two primary scenarios in which lind-3i must explicitly locate and
//! re-enter a different runtime state.
//!
//! The first scenario occurs during process-like operations such as `fork`, `exec`, and `exit`. These
//! operations require Wasmtime to create, clone, or destroy existing Wasm instances. After RawPOSIX
//! completes the semantic handling of a `fork`, `exec`, or `exit` operation, execution must return to
//! Wasmtime to continue running Wasm code. Importantly, the cage that performs the `fork`, `exec`, or
//! `exit` logic is not necessarily the same cage or grate that originally issued the system call. As
//! a result, lind-3i cannot rely on an implicit “current” runtime state. Instead, it must be able to
//! retrieve the Wasmtime execution context associated with an arbitrary `(cage_id, thread_id)` pair.
//!
//! The second scenario arises during grate calls. Grate calls involve cross-module execution transfers,
//! where control jumps from one Wasm module to another (for example, from a cage into a grate, or between
//! grates). Supporting these jumps similarly requires the ability to locate and enter the runtime state
//! of a different module than the one currently executing.
//!
//! To support both scenarios, lind-3i leverages a key property of lind-wasm’s execution model: each Wasmtime
//! `Store` contains exactly one Wasm `Instance`, and each thread executes within its own independent
//! store / instance pair.
//! At module creation time, lind-3i extracts the `VMContext` pointer associated with the newly created instance.
//! This `VMContext` uniquely identifies the execution state of that `Store` / `Instance`. The pointer is
//! stored in a global table indexed by `(cage_id, thread_id)`. When lind-3i needs to transfer execution to
//! another cage or grate, it looks up the corresponding `VMContext` pointer using the target `(cage_id, thread_id)`.
//! Using Wasmtime’s internal mechanisms, the `VMContext` pointer can be used to recover the associated
//! `Store` and `Instance`, allowing execution to resume in the correct runtime context.
//!
//! The table intentionally stores raw `VMContext` pointers rather than typed store or instance handles.
//! This design avoids Rust lifetime constraints that would otherwise prevent cross-store and cross-instance
//! execution transfers. Correctness instead relies on higher-level invariants enforced by lind-wasm, including
//! the guarantee that `VMContext` pointers remain valid for the lifetime of their associated threads.
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::ffi::c_void;
use std::ptr::NonNull;
use threei::threei_const;

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

/// `CageId` represents a logical isolation domain (similar to a process).
type CageId = u64;
/// `ThreadId` represents a thread of execution within a Cage.
type ThreadId = u64;
/// `(CageId, ThreadId)` uniquely identify a running Wasm thread.
/// `CageThreadKey` is a convenience alias used as the key type in the global table.
type CageThreadKey = (CageId, ThreadId);

/// VMCTX_TABLE is a global concurrent hash map that stores the mapping:
/// (cage_id, thread_id) -> VmCtxWrapper.
lazy_static! {
    /// Global map: <(cage_id, thread_id), VmCtxWrapper>
    pub static ref VMCTX_TABLE: DashMap<CageThreadKey, VmCtxWrapper> = DashMap::new();
}

/// `get_vmctx` looks up the `VMContext` associated with a given cage and thread.
/// It returns a copy of the stored `VmCtxWrapper`, or `None` if no entry exists.
///
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
/// operating through a separate VmCtxW`rapper copy.
///
/// At present, lind-wasm does not enforce synchronization at this level. Grate developers are responsible for
/// ensuring proper concurrency control whenever their grate code mutates shared memory, for example by using
/// explicit locking, atomic operations, or other synchronization mechanisms appropriate to their execution model.
pub fn get_vmctx(cage_id: CageId, thread_id: ThreadId) -> Option<VmCtxWrapper> {
    VMCTX_TABLE.get(&(cage_id, thread_id)).map(|v| *v)
}

/// `set_vmctx`
/// (1) inserts or replaces the `VMContext` associated with a given cage and thread.
/// (2) It also notifies threei of the runtime type for the cage by calling `threei::set_cage_runtime`.
/// This is typically called when a Wasm thread starts executing or when its `VMContext` becomes available.
pub fn set_vmctx(cage_id: CageId, thread_id: ThreadId, vmctx: VmCtxWrapper) {
    // 1) Notify threei of the cage runtime type
    threei::set_cage_runtime(cage_id, threei_const::RUNTIME_WASMTIME);
    // 2) Insert the `VMContext` entry in the global table
    VMCTX_TABLE.insert((cage_id, thread_id), vmctx);
}

/// `rm_vmctx` removes the VMContext entry for a given cage and thread.
/// It returns the removed VmCtxWrapper if one was present.
/// This is typically called when a thread exits or its execution context is torn down.
pub fn rm_vmctx(cage_id: CageId, thread_id: ThreadId) -> Option<VmCtxWrapper> {
    VMCTX_TABLE.remove(&(cage_id, thread_id)).map(|(_, v)| v)
}
