use crate::threei_const;
use dashmap::DashMap;
use std::sync::Mutex;

/// HANDLERTABLE:
/// A nested hash map used to define fine-grained per-syscall interposition rules.
///
/// <self_cageid, <callnum, (addr, dest_grateid)>
/// Keys are the grate, the value is a HashMap with a key of the callnum
/// and the values are a (target_call_index, grate) tuple for the actual handlers...
type TargetCageMap = DashMap<u64, u64>; // Maps destfunc to dest_grateid
type CallnumMap = DashMap<u64, TargetCageMap>; // Maps targetcallnum to TargetCageMap
type CageHandlerTable = DashMap<u64, CallnumMap>; // Maps self_cageid to CallnumMap

lazy_static::lazy_static! {
    // <self_cageid, <callnum, (target_call_index, dest_grateid)>
    // callnum is mapped to addr, not self
    pub static ref HANDLERTABLE: CageHandlerTable = DashMap::new();
}

/// Checks if a given cage has any registered syscall handlers in HANDLERTABLE.
///
/// ## Arguments:
/// - cageid: The ID of the cage to check.
///
/// ## Returns:
/// true if the cage has at least one handler registered.
/// false otherwise.
pub fn _check_cage_handler_exist(cageid: u64) -> bool {
    HANDLERTABLE.contains_key(&cageid)
}

/// Looks up the destination grate and call index for a given syscall issued by a specific cage.
///
/// ## Arguments:
/// - self_cageid: ID of the calling cage.
/// - syscall_num: The syscall number issued by the cage.
///
/// ## Returns:
/// `Some((call_index_in_grate, dest_grateid))` if a handler mapping exists.
/// `None` if no mapping is found.
pub fn _get_handler(self_cageid: u64, syscall_num: u64) -> Option<(u64, u64)> {
    HANDLERTABLE
        .get(&self_cageid)?
        .get(&syscall_num)?
        .iter()
        .next()
        .map(|e| (*e.key(), *e.value()))
}

/// Removes **ALL** handler entries across all cages that point to a specific grateid.
///
/// Mutates the HANDLERTABLE by removing all handler mappings that route to this grate,
/// cleaning up stale references after removal or teardown.
///
/// ## Arguments:
/// - grateid: The ID of the grate to purge from the HANDLERTABLE.
///
/// ## Returns:
/// None.
///
/// todo: a more efficient way to do clean up
pub fn _rm_grate_from_handler(grateid: u64) {
    HANDLERTABLE.iter().for_each(|entry| {
        let callmap = entry.value();
        callmap.iter().for_each(|call_entry| {
            let target_map = call_entry.value();
            target_map.retain(|_, dest_grateid| *dest_grateid != grateid);
        });
    });
}

/// Removes **all** handler mappings registered under a given cage.
///
/// This function deletes the entire entry for the specified `cageid` in the
/// global `HANDLERTABLE`. After this call, the cage will have no syscall
/// interposition rules associated with it.
///
/// ## Arguments:
/// - `cageid`: The ID of the cage to remove.
///
/// ## Returns:
/// None.
pub fn _rm_cage_from_handler(cageid: u64) {
    // Remove cage's own handler table if it exists
    HANDLERTABLE.remove(&cageid);
}

/// Actual implementation of register_handler.
/// See comments in threei.rs for details.
pub fn register_handler_impl(
    targetcage: u64,
    targetcallnum: u64,
    handlefunc: u64,
    handlefunccage: u64,
) -> i32 {
    // If `handlefunccage == THREEI_DEREGISTER`, remove the entire callnum entry
    // for the given (targetcage, targetcallnum).
    // We assume one (targetcage, targetcallnum) could be mapped to multiple (handlefunc, handlefunccage)
    // and each time calling will check the handlefunccage to determine the destination.
    if handlefunccage == threei_const::THREEI_DEREGISTER {
        let mut should_remove_cage = false;
        if let Some(cage_entry) = HANDLERTABLE.get(&targetcage) {
            cage_entry.remove(&targetcallnum);
            should_remove_cage = cage_entry.value().is_empty();
            drop(cage_entry);
            // drop the borrow to cage_entry before mutating handler_table again
            if should_remove_cage {
                HANDLERTABLE.remove(&targetcage);
            };
        }

        return 0;
    }

    if let Some(mut cage_entry) = HANDLERTABLE.get_mut(&targetcage) {
        let mut should_remove_cage = false;
        // Check if targetcallnum exists
        if let Some(mut callnum_entry) = cage_entry.get_mut(&targetcallnum) {
            // （targetcage, targetcallnum) exists
            if handlefunc == 0 {
                // If deregistering a single syscall, remove the entry if it exists
                callnum_entry.retain(|_, dest_grateid| *dest_grateid != handlefunccage);
                // cleanup empties
                let empty_callnum = callnum_entry.is_empty();
                drop(callnum_entry);
                if empty_callnum {
                    cage_entry.remove(&targetcallnum);
                    should_remove_cage = cage_entry.is_empty();
                    if should_remove_cage {
                        HANDLERTABLE.remove(&targetcage);
                    }
                }
                return 0;
            }

            match callnum_entry.get(&handlefunc) {
                Some(existing_dest_grateid) if *existing_dest_grateid == handlefunccage => {
                    // Already registered with same mapping, do nothing
                    return 0;
                }
                Some(_) => {
                    return threei_const::ELINDAPIABORTED as i32; // Return error if a conflicting mapping exists
                }
                None => {
                    // If `handlefunc` not exists, insert
                    callnum_entry.insert(handlefunc, handlefunccage);
                    return 0;
                }
            }
        } else {
            // callnum does not exist yet under this cage
            if handlefunc == 0 {
                // nothing to delete
                return 0;
            }
            let mut m = DashMap::new();
            m.insert(handlefunc, handlefunccage);
            cage_entry.insert(targetcallnum, m);
            return 0;
        }

        return 0;
    }

    // cage does not exist yet
    // Inserts a new mapping in HANDLERTABLE.
    if handlefunc == 0 {
        // nothing to delete
        return 0;
    }

    let cage_entry = HANDLERTABLE.entry(targetcage).or_insert_with(DashMap::new);

    let callmap = cage_entry
        .value()
        .entry(targetcallnum)
        .or_insert_with(DashMap::new);

    callmap.insert(handlefunc, handlefunccage);

    0
}

/// Actual implementation of copy_handler_table_to_cage.
/// See comments in threei.rs for details.
pub fn copy_handler_table_to_cage_impl(srccage: u64, targetcage: u64) -> u64 {
    // If srccage has a handler table, clones its contents into targetcage.
    // Does not overwrite any existing handlers in the target.
    if let Some(src_entry_ref) = HANDLERTABLE.get(&srccage) {
        let src_entry = src_entry_ref.value();
        let target_entry_guard = HANDLERTABLE.entry(targetcage).or_insert_with(DashMap::new);
        let target_entry = target_entry_guard.value(); // Ensure the scope of lifetime is long enough

        for callnum_ref in src_entry.iter() {
            let callnum = callnum_ref.key();
            let callnum_map = callnum_ref.value();
            let target_callnum_map_guard =
                target_entry.entry(*callnum).or_insert_with(DashMap::new);
            let target_callnum_map = target_callnum_map_guard.value(); // Ensure the scope of lifetime is long enough
            for handlefunc_ref in callnum_map.iter() {
                // If not already present, insert
                let handlefunc = handlefunc_ref.key();
                let handlefunccage = handlefunc_ref.value();
                target_callnum_map
                    .entry(*handlefunc)
                    .or_insert(*handlefunccage);
            }
        }
        0
    } else {
        eprintln!(
            "[3i|copy_handler_table_to_cage] srccage {} has no handler table",
            srccage
        );
        threei_const::ELINDAPIABORTED as u64 // treat missing src table as an error
    }
}
