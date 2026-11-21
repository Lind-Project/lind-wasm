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
pub fn register_simple(
    targetcage: u64,
    targetcallnum: u64,
    handlefunccage: u64,
    in_grate_fn_ptr_u64: u64,
    _op_flag: u64,
) -> i32 {
    register_handler(
        0, // _self_cageid
        0, // _target_cageid
        targetcage,
        targetcallnum,
        0, // _runtime_id
        handlefunccage,
        in_grate_fn_ptr_u64,
        0, // _arg3cageid
        0, // _arg4
        0, // _arg4cageid
        0, // _arg5
        0, // _arg5cageid
        0, // _arg6
        0, // _arg6cageid
    )
}

pub fn cpy(target: u64, src: u64) -> u64 {
    copy_handler_table_to_cage(
        0,      // _thiscage
        0,      // _targetcage
        src,    // srccage
        target, // targetcage
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    )
}
