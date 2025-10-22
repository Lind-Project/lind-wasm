use once_cell::sync::Lazy;
use std::sync::RwLock;
use std::collections::HashMap;
use core::ffi::c_void;
use threei::GrateFnEntry;

pub const GRATE_OK: i32   = 0;
pub const GRATE_ERR: i32  = -1;

pub static GrateFn_WasmTable: Lazy<RwLock<HashMap<(u64, u64), Box<GrateFnEntry>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub unsafe extern "C" fn set_gratefn_wasm(pid: u64, entry: *const GrateFnEntry) -> i32 {
    if entry.is_null() { return -1; }
    let entry = *entry;
    if entry.ctx_ptr.is_null() { return -1; }
    let mut map = GrateFn_WasmTable.write().unwrap();
    map.insert((pid, 0), Box::new(entry));
    0
}

pub fn take_gratefn_wasm(pid: u64) -> Option<*const GrateFnEntry> {
    let map = GrateFn_WasmTable.read().unwrap();
    map.get(&(pid, 0)).map(|b| &**b as *const GrateFnEntry)
}

pub fn remove_ctx(pid: u64) {
    let mut map = GrateFn_WasmTable.write().unwrap();
    map.remove(&(pid, 0));
}