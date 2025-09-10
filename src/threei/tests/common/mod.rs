// ---------- Test helper functions ----------
use threei::{HANDLERTABLE, EXITING_TABLE, register_handler, copy_handler_table_to_cage};
/// Clear global tables so each test starts from a clean state.
pub fn clear_globals() {
    {
        let mut tbl = HANDLERTABLE.lock().unwrap();
        tbl.clear();
    }
    // If EXITING_TABLE is a set-like structure:
    EXITING_TABLE.clear();
}

/// Read current mapping for (cage, callnum) into a Vec<(handlefunc, dest)>
pub fn mappings_for(cage: u64, callnum: u64) -> Vec<(u64, u64)> {
    let tbl = HANDLERTABLE.lock().unwrap();
    if let Some(cage_entry) = tbl.get(&cage) {
        if let Some(callnum_entry) = cage_entry.get(&callnum) {
            return callnum_entry.iter().map(|(k, v)| (*k, *v)).collect();
        }
    }
    vec![]
}

/// Convenience to call register_handler with only the meaningful args.
pub fn reg(targetcage: u64, targetcallnum: u64, handlefunc: u64, handlefunccage: u64) -> i32 {
    register_handler(
        0,               // _callnum (unused)
        targetcage,      // target cage
        targetcallnum,   // syscall number
        0,               // _arg1cage
        handlefunc,      // handlefunc (or 0 for selective deregister)
        handlefunccage,  // dest cage / THREEI_DEREGISTER
        0,0,0,0,0,0,0,0, // remaining unused args
    )
}

pub fn cpy(target: u64, src: u64) -> u64 {
    copy_handler_table_to_cage(0, target, src, 0, 0,0,0,0,0,0,0,0,0,0)
}
