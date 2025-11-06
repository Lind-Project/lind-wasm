use crate::threei_const;
use std::collections::HashMap;
use std::sync::Mutex;

/// HANDLERTABLE:
/// A nested hash map used to define fine-grained per-syscall interposition rules.
///
/// <self_cageid, <callnum, (in_grate_addr, dest_grateid)>
/// Keys are the grate, the value is a HashMap with a key of the callnum
/// and the values are a (in_grate_addr, grate) tuple for the actual handlers...
type TargetCageMap = HashMap<u64, u64>; // Maps destfunc in grate addr to dest_grateid
type CallnumMap = HashMap<u64, TargetCageMap>; // Maps targetcallnum to TargetCageMap
type CageHandlerTable = HashMap<u64, CallnumMap>; // Maps self_cageid to CallnumMap

lazy_static::lazy_static! {
    // <self_cageid, <callnum, (in_grate_addr, dest_grateid)>
    // callnum is mapped to addr, not self
    pub static ref HANDLERTABLE: Mutex<CageHandlerTable> = Mutex::new(HashMap::new());
}

/// Helper function for debugging.
/// Prints the current contents of `HANDLERTABLE` in a readable format
/// to help inspect cage–callnum–target mappings during development.
pub fn print_handler_table() {
    let table = HANDLERTABLE.lock().unwrap();
    println!("=== HANDLERTABLE ===");
    for (self_cageid, callnum_map) in table.iter() {
        println!("CageID: {}", self_cageid);
        for (callnum, target_map) in callnum_map.iter() {
            println!("  Callnum: {}", callnum);
            for (destfunc, dest_grateid) in target_map.iter() {
                println!(
                    "    destfunc: {} -> dest_grateid: {}",
                    destfunc, dest_grateid
                );
            }
        }
    }
    println!("====================");
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
    let handler_table = HANDLERTABLE.lock().unwrap();
    handler_table.contains_key(&cageid)
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
    let handler_table = HANDLERTABLE.lock().unwrap();

    handler_table
        .get(&self_cageid) // Get the first HashMap<u64, HashMap<u64, u64>>
        .and_then(|sub_table| sub_table.get(&syscall_num)) // Get the second HashMap<u64, u64>
        .and_then(|map| map.iter().next()) // Extract the first (key, value) pair
        .map(|(&call_index, &grateid)| (call_index, grateid)) // Convert to (u64, u64)
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
    let mut table = HANDLERTABLE.lock().unwrap();
    for (_, callmap) in table.iter_mut() {
        for (_, target_map) in callmap.iter_mut() {
            target_map.retain(|_, &mut dest_grateid| dest_grateid != grateid);
        }
    }
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
    let mut handler_table = HANDLERTABLE.lock().unwrap();
    handler_table.remove(&cageid);
}

/// Actual implementation of register_handler.
/// See comments in threei.rs for details.
pub fn register_handler_impl(
    targetcage: u64,
    targetcallnum: u64,
    is_register: u64, // 0: deregister, otherwise: register
    handlefunccage: u64,
    in_grate_fn_ptr_u64: u64,
) -> i32 {
    let mut handler_table = HANDLERTABLE.lock().unwrap();

    // If `handlefunccage == THREEI_DEREGISTER`, remove the entire callnum entry
    // for the given (targetcage, targetcallnum).
    // We assume one (targetcage, targetcallnum) could be mapped to multiple (handlefunc, handlefunccage)
    // and each time calling will check the handlefunccage to determine the destination.
    if handlefunccage == threei_const::THREEI_DEREGISTER {
        let mut should_remove_cage = false;
        if let Some(cage_entry) = handler_table.get_mut(&targetcage) {
            cage_entry.remove(&targetcallnum);
            should_remove_cage = cage_entry.is_empty();
        }
        // drop the borrow to cage_entry before mutating handler_table again
        if should_remove_cage {
            handler_table.remove(&targetcage);
        }
        return 0;
    }

    if let Some(cage_entry) = handler_table.get_mut(&targetcage) {
        // Check if targetcallnum exists
        if let Some(callnum_entry) = cage_entry.get_mut(&targetcallnum) {
            // （targetcage, targetcallnum) exists
            if is_register == 0 {
                // If deregistering a single syscall, remove the entry if it exists
                callnum_entry.retain(|_, dest_grateid| *dest_grateid != handlefunccage);
                // cleanup empties
                let empty_callnum = callnum_entry.is_empty();
                if empty_callnum {
                    // end borrow of callnum_entry by scoping
                    // remove callnum, then possibly remove cage
                    // (cannot hold &mut to value while mutating parent)
                    cage_entry.remove(&targetcallnum);
                    if cage_entry.is_empty() {
                        handler_table.remove(&targetcage);
                    }
                }
                return 0;
            }

            match callnum_entry.get(&in_grate_fn_ptr_u64) {
                Some(existing_dest_grateid) if *existing_dest_grateid == handlefunccage => {
                    // Already registered with same mapping, do nothing
                    return 0;
                }
                Some(_) => {
                    return threei_const::ELINDAPIABORTED as i32; // Return error if a conflicting mapping exists
                }
                None => {
                    // If `in_grate_fn_ptr` not exists, insert
                    callnum_entry.insert(in_grate_fn_ptr_u64, handlefunccage);
                    return 0;
                }
            }
        } else {
            // callnum does not exist yet under this cage
            if is_register == 0 {
                // nothing to delete
                return 0;
            }
            let mut m = HashMap::new();
            m.insert(in_grate_fn_ptr_u64, handlefunccage);
            cage_entry.insert(targetcallnum, m);
            return 0;
        }
    }

    // cage does not exist yet
    // Inserts a new mapping in HANDLERTABLE.
    if is_register == 0 {
        // nothing to delete
        return 0;
    }

    handler_table
        .entry(targetcage)
        .or_insert_with(HashMap::new)
        .entry(targetcallnum)
        .or_insert_with(HashMap::new)
        .insert(in_grate_fn_ptr_u64, handlefunccage);

    0
}

/// Actual implementation of copy_handler_table_to_cage.
/// See comments in threei.rs for details.
pub fn copy_handler_table_to_cage_impl(srccage: u64, targetcage: u64) -> u64 {
    let mut handler_table = HANDLERTABLE.lock().unwrap();

    // If srccage has a handler table, clones its contents into targetcage.
    // Does not overwrite any existing handlers in the target.
    if let Some(src_entry) = handler_table.get(&srccage).cloned() {
        let target_entry = handler_table.entry(targetcage).or_insert_with(HashMap::new);
        for (callnum, callnum_map) in src_entry {
            let target_callnum_map = target_entry.entry(callnum).or_insert_with(HashMap::new);
            for (handlefunc, handlefunccage) in callnum_map {
                // If not already present, insert
                target_callnum_map
                    .entry(handlefunc)
                    .or_insert(handlefunccage);
            }
        }
        0
    } else {
        eprintln!(
            "[3i|copy_handler_table_to_cage] srccage {} has no handler table",
            srccage
        );
        threei_const::ELINDAPIABORTED // treat missing src table as an error
    }
}
