use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn lib_symbol_table() -> &'static Mutex<HashMap<u64, HashMap<(String, String), u64>>> {
    static TABLE: OnceLock<Mutex<HashMap<u64, HashMap<(String, String), u64>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a (lib_name, symbol_name) -> call_id mapping for target_cage_id.
pub fn register_lib_symbol(cage_id: u64, lib_name: &str, symbol_name: &str, call_id: u64) {
    let mut table = lib_symbol_table().lock().unwrap();
    table
        .entry(cage_id)
        .or_default()
        .insert((lib_name.to_string(), symbol_name.to_string()), call_id);
}

/// Look up the call_id for (cage_id, lib_name, symbol_name).
/// Returns None if no handler has been registered for this symbol.
pub fn get_lib_call_id(cage_id: u64, lib_name: &str, symbol_name: &str) -> Option<u64> {
    let table = lib_symbol_table().lock().unwrap();
    table
        .get(&cage_id)?
        .get(&(lib_name.to_string(), symbol_name.to_string()))
        .copied()
}

/// Remove all lib symbol entries for cage_id. Called on cage exit/cleanup.
pub fn rm_cage_from_lib_symbol_table(cage_id: u64) {
    let mut table = lib_symbol_table().lock().unwrap();
    table.remove(&cage_id);
}

/// Copy all lib symbol entries from src_cage_id to dst_cage_id.
/// Called on fork so the child cage inherits the parent's registered handlers.
pub fn copy_lib_symbol_table_to_cage(src_cage_id: u64, dst_cage_id: u64) {
    let mut table = lib_symbol_table().lock().unwrap();
    if let Some(src_map) = table.get(&src_cage_id).cloned() {
        table.insert(dst_cage_id, src_map);
    }
}
