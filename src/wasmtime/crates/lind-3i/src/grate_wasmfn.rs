//! This module provides runtime-specific storage and access for `GrateFn` closures.
//!
//! The `register_handler` API is intended to be reusable across different runtimes.
//! To support this, it accepts an explicit argument representing the runtime-specific
//! entry point. For Wasmtime, this is implemented as a Rust closure (`GrateFn`).
//!
//! This file maintains a per-grate table of these closures inside Wasmtime.  
//! Each Wasm instance can register its own re-entry closure via
//! `set_gratefn_wasm()`, which stores it in the global `GrateFn_WasmTable`.
//!
//! Later, when `register_handler` is invoked within `lind-common`, it uses the
//! grate index to look up the corresponding closure and re-enter Wasmtime
//! execution. The invocation is then routed through 3i, where the handler table
//! records and dispatches the call according to the registration metadata.
use sysdefs::constants::lind_platform_const;

type GrateFn =
    dyn FnMut(
        u64, u64, u64, u64, u64, u64, u64,
        u64, u64, u64, u64, u64, u64, u64
    ) -> i32;

/// `GrateFn_WasmTable`: central registry for re-entry closures associated with grate IDs.
static mut GrateFn_WasmTable: Option<
    Vec<
        Option<
            Box<GrateFn>,
        >,
    >,
> = None;

/// Initializes the GrateFn_WasmTable with a capacity for MAX_CAGEID entries.
fn _init_gratefn_wasm() {
    unsafe {
        let vec = GrateFn_WasmTable
            .get_or_insert_with(|| Vec::with_capacity(lind_platform_const::MAX_CAGEID as usize));
        vec.resize_with(lind_platform_const::MAX_CAGEID as usize, || None);
    }
}

/// `set_gratefn_wasm()` stores the runtime entry closure for each grate ID  
pub fn set_gratefn_wasm(
    grateid: u64,
    mut callback: Box<GrateFn>,
) -> i32 {
    let index = grateid as usize;
    unsafe {
        if GrateFn_WasmTable.is_none() {
            _init_gratefn_wasm();
        }

        if let Some(ref mut vec) = GrateFn_WasmTable {
            if index < vec.len() {
                vec[index] = Some(callback);
            } else {
                panic!("[3i|set_gratefn_wasm] Index out of bounds: {}", index);
            }
        }
    }

    0
}

/// `take_gratefn_wasm()` retrieves the closure for the given grate ID, removing it from the table.
pub fn take_gratefn_wasm(grateid: usize) -> Box<GrateFn> {
    unsafe {
        let table = GrateFn_WasmTable
            .as_mut()
            .expect("GrateFn_WasmTable not initialized");
        if grateid >= table.len() {
            panic!("grateid {} out of bounds (len={})", grateid, table.len());
        }
        table[grateid]
            .take()
            .expect("no function stored at grateid")
    }
}
