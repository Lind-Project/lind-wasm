use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn lib_handler_table() -> &'static Mutex<HashMap<u64, HashMap<(String, String), (u64, u64)>>> {
    static TABLE: OnceLock<Mutex<HashMap<u64, HashMap<(String, String), (u64, u64)>>>> =
        OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a (lib_name, symbol_name) → (handler_cage_id, fn_ptr) mapping for cage_id.
pub fn register_lib_handler_entry(
    cage_id: u64,
    lib_name: &str,
    symbol_name: &str,
    handler_cage_id: u64,
    fn_ptr: u64,
) {
    let mut table = lib_handler_table().lock().unwrap();
    table.entry(cage_id).or_default().insert(
        (lib_name.to_string(), symbol_name.to_string()),
        (handler_cage_id, fn_ptr),
    );
}

/// Look up (handler_cage_id, fn_ptr) for (cage_id, lib_name, symbol_name).
/// Returns None if no handler has been registered for this symbol.
pub fn get_lib_handler(cage_id: u64, lib_name: &str, symbol_name: &str) -> Option<(u64, u64)> {
    let table = lib_handler_table().lock().unwrap();
    table
        .get(&cage_id)?
        .get(&(lib_name.to_string(), symbol_name.to_string()))
        .copied()
}

/// Remove all lib handler entries for cage_id. Called on cage exit/cleanup.
pub fn rm_cage_from_lib_handler_table(cage_id: u64) {
    let mut table = lib_handler_table().lock().unwrap();
    table.remove(&cage_id);
}

/// Copy all lib handler entries from src_cage_id to dst_cage_id.
/// Called on fork so the child cage inherits the parent's registered handlers.
pub fn copy_lib_handler_table_to_cage(src_cage_id: u64, dst_cage_id: u64) {
    let mut table = lib_handler_table().lock().unwrap();
    if let Some(src_map) = table.get(&src_cage_id).cloned() {
        table.insert(dst_cage_id, src_map);
    }
}
