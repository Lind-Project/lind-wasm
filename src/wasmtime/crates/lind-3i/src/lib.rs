//! This module provides a runtime-local staging table for 3i re-entry metadata when using Wasmtime.
//! Because we want 3i’s public API to remain runtime-agnostic (so it can be adapted to multiple
//! runtimes in the future), the actual attachment of the callback function pointer and its
//! `VMContext` pointer is deferred to the `crates::lind-common` layer when it reroutes `register_handler`.
//!
//! During module initialization we capture the target instance’s `VMContext`, but there can be a
//! time gap between:
//! (a) initialization
//! (b) the user’s Wasm code calling `register_handler`.
//! To bridge this gap, Wasmtime keeps a per-(pid, tid) entry in a local table here.
//!
//! When `register_handler` reaches `crates::lind-common` and is forwarded to 3i, we extract the
//! staged entry pointer from this table and pass it along to 3i as the canonical source of
//! callback + context for the target Grate.
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use threei::{threei_const, GrateFnEntry};

/// The [`VMContext`](wasmtime_runtime::VMContext) is Wasmtime’s low-level runtime state for an
/// instance. It includes the instance’s memories, tables, globals,
/// and other execution state needed when entering or re-entering Wasm
/// code. For more details, see the documentation in `wasmtime_runtime::vmcontext`.
///
/// This is used in 3i to support cross-instance calls, allowing syscalls from one
/// cage to invoke functions in another cage. For example, when a syscall from cage A is
/// routed to a function in grate B, we need to look up grate B’s runtime context in order
/// to call the closure inside it.
///
/// The runtime context includes a pointer to the instance’s `VMContext`, which is required
/// by Wasmtime to correctly re-enter the target instance with the right execution state.
/// `GRATE_FN_WASM_TABLE` is a `HashMap<(u64, u64), Box<GrateFnEntry>>` keyed by `(pid, tid)`
/// that stores one entry per process/thread.
///
/// Todo: We currently use tid = 0 as a placeholder; the multi-thread support is needed in future.
///
/// Entries are boxed to keep their addresses stable—3i receives and holds raw pointers to these
/// entries, so using `Box` ensures pointer validity even if the map rehashes or moves its buckets.
pub static GRATE_FN_WASM_TABLE: Lazy<RwLock<HashMap<(u64, u64), Box<GrateFnEntry>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Called during module initiation (from [`wasmtime_cli::run.rs`]).
/// Builds a `GrateFnEntry` from Wasmtime-side state, validates that both the entry pointer and
/// its `ctx_ptr` are non-null, and stores the boxed entry into `GRATE_FN_WASM_TABLE` under `(pid, 0)`.
///
/// Returns `GRATE_OK` on success or `GRATE_ERR` on invalid input.
///
/// This function is `extern "C"` and `unsafe` because it crosses the FFI boundary and dereferences
/// raw pointers.
pub unsafe extern "C" fn set_gratefn_wasm(pid: u64, entry: *const GrateFnEntry) -> i32 {
    if entry.is_null() {
        return threei_const::GRATE_ERR;
    }
    let entry = *entry;
    if entry.ctx_ptr.is_null() {
        return threei_const::GRATE_ERR;
    }
    let mut map = GRATE_FN_WASM_TABLE.write().unwrap();
    map.insert((pid, 0), Box::new(entry));

    threei_const::GRATE_OK
}

/// Used by [`lind-common::register_handler`] to fetch the previously staged entry and pass its pointer into 3i.
/// Performs a read-locked lookup of `(pid, 0)` in `GRATE_FN_WASM_TABLE`  
///
/// Returns a raw `*const GrateFnEntry` for 3i to consume.
///
/// No ownership is transferred; the entry remains owned by this module until cleanup.
pub fn take_gratefn_wasm(pid: u64) -> Option<*const GrateFnEntry> {
    let map = GRATE_FN_WASM_TABLE.read().unwrap();
    map.get(&(pid, 0)).map(|b| &**b as *const GrateFnEntry)
}

/// Called when the Wasm module (or its Grate instance) exits.
/// Removes the `(pid, 0)` entry from `GRATE_FN_WASM_TABLE`
pub fn remove_gratefn_wasm(pid: u64) -> bool {
    let mut map = GRATE_FN_WASM_TABLE.write().unwrap();
    map.remove(&(pid, 0));
    true
}
