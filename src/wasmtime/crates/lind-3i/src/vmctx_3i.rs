//! This crate extracts the operations on the global `VM_TABLE`
//! (a registry of `InstanceHandle`s for running Wasm instances) into
//! a standalone module.
//!
//! The motivation is to avoid circular dependencies between crates.
//!
//! For example, when a cage exits, its entry in the `VM_TABLE` must be
//! removed. Both `wasmtime-cli` and `lind-multi-process` crates need to call
//! this functionality. By moving the table management into its own
//! crate, they can depend on this crate instead of depending on each
//! other, preventing circular imports.

use once_cell::sync::Lazy;
use std::sync::RwLock;
use wasmtime::InstanceHandle;

/// `VM_TABLE` stores the runtime context [`InstanceHandle`](wasmtime::InstanceHandle)
/// of each running Wasm instance, indexed by the instance's ID (`cageid`).
///
/// An `InstanceHandle` in Wasmtime owns the compiled instance and
/// provides access to its [`VMContext`](wasmtime_runtime::VMContext).
/// The `VMContext` is Wasmtime’s low-level runtime state for an
/// instance. It includes the instance’s memories, tables, globals,
/// and other execution state needed when entering or re-entering Wasm
/// code. For more details, see the documentation in `wasmtime_runtime::vmcontext`.
///
/// This is used in 3i to support cross-instance closure calls, allowing syscalls from one
/// cage to invoke functions in another cage. For example, when a syscall from cage A is
/// routed to a function in grate B, we need to look up grate B’s runtime context in order
/// to call the closure inside it.
///
/// The runtime context includes a pointer to the instance’s `VMContext`, which is required
/// by Wasmtime to correctly re-enter the target instance with the right execution state.
///
/// - `insert_ctx(cageid, ctx)` is called during instance initialization to register its context.
/// - `get_ctx(cageid)` retrieves the context by `cageid`, and uses `unsafe { ctx.clone() }`
///   to manually clone the handle for invocation.
/// - `remove_ctx(cageid)` removes the context when the instance is terminated. Returns `true`
///   if the context was found and removed, `false` otherwise.
///
/// ## Lifetime
/// `InstanceHandle` directly holds a raw pointer to its `VMContext`. That `VMContext` is
/// allocated when the instance is created (e.g., via `Instance::new` / `Instance::new_with_handle`)
/// and remains valid only as long as the owning `Store` lives. In other words, an `InstanceHandle`’s
/// effective lifetime is bound to the lifetime of its `Store`.
///
/// In lind-wasm, the `Store` lives for the duration of `wasmtime_cli::run::{execute, execute_with_lind}`.
/// To avoid leaving dangling references after the `Store` is dropped, we explicitly remove
/// and drop each module’s `InstanceHandle` on `exit_syscall` (See `lind-multi-processes` and `wasmtime-cli::run`).
/// While Wasmtime itself will reclaim all instances when the `Store` is dropped, this
/// cleanup ensures that no global tables or caches retain stale handles beyond the `Store`’s
/// lifetime, preventing illegal access to freed engine resources.
pub static VM_TABLE: Lazy<RwLock<Vec<Option<InstanceHandle>>>> =
    Lazy::new(|| RwLock::new(Vec::new()));

/// Inserts a new context into the global VM_TABLE at the specified cageid.
pub fn insert_ctx(cageid: usize, ctx: InstanceHandle) {
    let mut table = VM_TABLE.write().unwrap();
    if cageid >= table.len() {
        table.resize(cageid + 1, None);
    }
    table[cageid] = Some(ctx);
}

/// Retrieves the context for the given cageid.
pub fn get_ctx(cageid: usize) -> InstanceHandle {
    let table = VM_TABLE.read().unwrap();
    let ctx = table[cageid].as_ref().unwrap();
    // SAFETY: `InstanceHandle` cloning is `unsafe` because it may lead to VMContext aliasing
    // if not properly managed. Here, we assume the cloned context is only used temporarily
    // and not stored beyond the scope of the call.
    unsafe { ctx.clone() }
}

/// Removes the context for the given cageid.
/// Returns true if the context was found and removed, false otherwise.
pub fn remove_ctx(cageid: usize) -> bool {
    let mut table = VM_TABLE.write().unwrap();
    if cageid < table.len() {
        table[cageid] = None;
        return true;
    }
    false
}
