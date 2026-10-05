//! Gate 0 (cross-cage function-pointer callbacks) scaffolding: lets a V2
//! grate call re-enter the calling cage's own `Store` to invoke a function
//! pointer the caller passed in, while that caller is suspended partway
//! through the same synchronous dispatch that is still on this OS thread's
//! call stack.
//!
//! See `local-notes/active/plan-cross-cage-function-pointers.md`'s "Re-entry
//! model" for the invariants this must uphold. The short version: the V2
//! portal closure in `linker.rs` already holds a live, exclusive
//! `StoreContextMut` for the calling cage (A) for the entire duration of the
//! nested dispatch into the grate (B) -- `dispatch_lib_call_v2` does not
//! return until every nested call, including any callback invocation, has
//! finished. This module lets that SAME borrow be reused, not re-acquired,
//! from deep inside that nested call, by threading it through a thread-local
//! stack as a raw pointer instead of a typed reference: the intervening
//! layers (`threei`, the `extern "C"` trampoline) are deliberately generic-
//! and Wasmtime-free, so a typed reference cannot pass through them as an
//! ordinary function parameter.
//!
//! Soundness rests entirely on strict LIFO nesting, not on anything the
//! compiler checks for us: `ActiveFrameGuard::push` is called immediately
//! before the call that lends the borrow out, `Drop` pops it immediately
//! after that call returns (even on panic/trap unwinding), and the ONLY
//! code that ever dereferences the stored pointer (`with_active_frame`) can
//! only run while that same call is still on the stack -- a callback proxy
//! is only ever invoked as part of the grate's own Wasm execution, which
//! itself only runs inside that window. No other code path independently
//! acquires a second mutable reference to the same `Store`.

use crate::prelude::*;
use crate::{StoreContextMut, StoreInner, Table};
use core::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Per-cage registry of the cage's own shared indirect-function table.
///
/// A dylink cage's main module is built with `--import-table`: the table is
/// shared across the main module and every preloaded library loaded into
/// that same cage (grown as each library is linked in, see
/// `Linker::module_with_preload`'s caller in `execute.rs`), so no single
/// module instance in the cage locally EXPORTS it -- `Caller::get_export`
/// would find nothing. This registry is populated once, directly from the
/// `Table` the boot sequence already creates via `attach_function_table`,
/// so `with_active_frame`'s caller doesn't need to guess which instance (if
/// any) happens to re-export it.
///
/// Gate 0 scaffolding: populated only by the initial cage-boot path
/// (`execute_with_lind`/`execute_wasmtime`) today, not yet by fork/exec/
/// thread re-attachment -- those are Gate 5's "lifecycle" scope, not this
/// probe's.
static CAGE_TABLES: OnceLock<Mutex<HashMap<u64, Table>>> = OnceLock::new();

fn cage_tables() -> &'static Mutex<HashMap<u64, Table>> {
    CAGE_TABLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records `table` as `cageid`'s own shared indirect-function table.
pub fn register_cage_table(cageid: u64, table: Table) {
    cage_tables().lock().unwrap().insert(cageid, table);
}

/// Looks up the indirect-function table previously recorded for `cageid`.
pub fn get_cage_table(cageid: u64) -> Option<Table> {
    cage_tables().lock().unwrap().get(&cageid).copied()
}

struct Frame<T: 'static> {
    /// The calling cage's own indirect-function table, resolved once at
    /// push time (a `Table` handle is a lightweight, `Copy`, store-bound
    /// index -- it carries no borrow of its own and is safe to stash).
    table: Table,
    /// Erased from the live `&mut StoreInner<T>` behind the caller's
    /// `StoreContextMut` for the duration of this frame. See the module
    /// doc for why this must be a raw pointer rather than a reference.
    store_ptr: *mut StoreInner<T>,
}

thread_local! {
    /// Stack of currently-suspended callers, most recent (innermost) last.
    /// A `Vec` rather than a single slot: nested A-to-B-to-C dispatch pushes
    /// more than one frame before any pops, one per cage currently
    /// suspended on this same OS thread's call stack.
    static ACTIVE_FRAMES: RefCell<Vec<(u64, Box<dyn Any>)>> = RefCell::new(Vec::new());
}

/// RAII guard for one pushed frame. Must be created immediately before the
/// call that will (transitively) use it, and dropped immediately after that
/// call returns -- see the module doc's nesting argument.
pub struct ActiveFrameGuard {
    cageid: u64,
}

impl ActiveFrameGuard {
    /// Pushes a re-entry frame for `cageid`, capturing `table` (the cage's
    /// own indirect-function table export) and reborrowing `store` as a raw
    /// pointer that `with_active_frame` can later reconstruct.
    pub fn push<T: 'static>(cageid: u64, table: Table, store: &mut StoreContextMut<'_, T>) -> Self {
        let store_ptr: *mut StoreInner<T> = &mut *store.0 as *mut StoreInner<T>;
        let frame = Frame::<T> { table, store_ptr };
        ACTIVE_FRAMES.with(|frames| frames.borrow_mut().push((cageid, Box::new(frame))));
        ActiveFrameGuard { cageid }
    }
}

impl Drop for ActiveFrameGuard {
    fn drop(&mut self) {
        ACTIVE_FRAMES.with(|frames| {
            let popped = frames.borrow_mut().pop();
            debug_assert!(
                popped.is_some_and(|(c, _)| c == self.cageid),
                "active-frame stack popped out of LIFO order"
            );
        });
    }
}

/// Re-enters the most recent active frame for `cageid`, calling `f` with a
/// fresh `StoreContextMut` and that cage's own `Table` handle. Returns
/// `None` if there is no active frame for `cageid` on this thread right
/// now -- a stale, forged, or cross-thread cage id must reject cleanly
/// rather than silently doing nothing or aliasing unrelated state. The
/// innermost (most recently pushed) matching frame is used, matching normal
/// call-stack shadowing if the same cage were (erroneously) re-entrant.
pub fn with_active_frame<T: 'static, R>(
    cageid: u64,
    f: impl FnOnce(StoreContextMut<'_, T>, Table) -> R,
) -> Option<R> {
    let found = ACTIVE_FRAMES.with(|frames| {
        frames
            .borrow()
            .iter()
            .rev()
            .find(|(c, _)| *c == cageid)
            .and_then(|(_, boxed)| {
                boxed
                    .downcast_ref::<Frame<T>>()
                    .map(|fr| (fr.table, fr.store_ptr))
            })
    })?;
    let (table, store_ptr) = found;
    // SAFETY: see the module doc. `store_ptr` was derived from a borrow that
    // is still alive (the pushing call has not returned) and exclusively
    // ours to reuse: no other live reference to the same `StoreInner<T>`
    // exists while this frame is on the stack.
    let store_ref: &mut StoreInner<T> = unsafe { &mut *store_ptr };
    Some(f(StoreContextMut(store_ref), table))
}
