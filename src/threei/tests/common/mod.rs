// ---------- Test helper functions ----------
use threei::handler_table::HANDLERTABLE;
use threei::{copy_handler_table_to_cage, register_handler, EXITING_TABLE};
/// Clear global tables so each test starts from a clean state.
pub fn clear_globals() {
    #[cfg(feature = "hashmap")]
    {
        HANDLERTABLE.lock().unwrap().clear();
    }
    #[cfg(feature = "dashmap")]
    {
        HANDLERTABLE.clear();
    }
    // If EXITING_TABLE is a set-like structure:
    EXITING_TABLE.clear();
}

/// Read current mapping for (cage, callnum) into a Vec<(handlefunc, dest)>
pub fn mappings_for(cage: u64, callnum: u64) -> Vec<(u64, u64)> {
    #[cfg(feature = "hashmap")]
    {
        let tbl = HANDLERTABLE.lock().unwrap();
        if let Some(cage_entry) = tbl.get(&cage) {
            if let Some(callnum_entry) = cage_entry.get(&callnum) {
                return callnum_entry.iter().map(|(k, v)| (*k, *v)).collect();
            }
        }
    }

    #[cfg(feature = "dashmap")]
    {
        if let Some(cage_entry_ref) = HANDLERTABLE.get(&cage) {
            let cage_entry = cage_entry_ref.value();
            if let Some(callnum_entry_ref) = cage_entry.get(&callnum) {
                let callnum_entry = callnum_entry_ref.value();
                return callnum_entry
                    .iter()
                    .map(|kv| (*kv.key(), *kv.value()))
                    .collect();
            }
        }
    }

    vec![]
}

/// Convenience to call register_handler with only the meaningful args.
pub fn reg(
    targetcage: u64,
    targetcallnum: u64,
    handlefunc: u64,
    handlefunccage: u64,
    op_flag: u64,
) -> i32 {
    register_handler(
        handlefunc,     // in_grate_fn_ptr_u64, we use handlefunc as a stand-in
        targetcage,     // target cage
        targetcallnum,  // syscall number
        0,              // _arg1cage
        op_flag,        // flag (or 0 for selective deregister)
        handlefunccage, // dest cage / THREEI_DEREGISTER
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0, // remaining unused args
    )
}

pub fn cpy(target: u64, src: u64) -> u64 {
    copy_handler_table_to_cage(0, target, src, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
}
