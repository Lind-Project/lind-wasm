use crate::threei_const;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::Mutex;
use sysdefs::constants::lind_platform_const;
use sysdefs::lind_log;

/// HANDLERTABLE:
/// A nested hash map used to define fine-grained per-syscall interposition rules.
///
/// HANDLERTABLE[self_cageid][callnum][target_cageid] =
///     (handler_cageid, handler_addr)
///
/// The innermost key is the target cage requested by `make_syscall`.
/// The value records which cage owns the handler function and the raw
/// function pointer to dispatch to.
type TargetCageMap = HashMap<u64, (u64, u64)>; // target id -> (handler cage id, handler addr)
type CallnumMap = HashMap<u64, TargetCageMap>; // Maps targetcallnum to TargetCageMap
type CageHandlerTable = HashMap<u64, CallnumMap>; // Maps self_cageid to CallnumMap

lazy_static::lazy_static! {
    // <self_cageid, <callnum, <target_cageid, (handler_cageid, handler_addr)>>>
    pub static ref HANDLERTABLE: Mutex<CageHandlerTable> = Mutex::new(HashMap::new());
}

/// Helper function for debugging.
/// Prints the current contents of `HANDLERTABLE` in a readable format
/// to help inspect cage–callnum–target mappings during development.
pub fn print_handler_table() {
    let table = HANDLERTABLE.lock().unwrap();
    lind_log!(THREEI, "=== HANDLERTABLE ===");
    for (self_cageid, callnum_map) in table.iter() {
        lind_log!(THREEI, "CageID: {}", self_cageid);
        for (callnum, target_map) in callnum_map.iter() {
            lind_log!(THREEI, "  Callnum: {}", callnum);
            for (target_id, (dest_grateid, in_grate_addr)) in target_map.iter() {
                lind_log!(
                    THREEI,
                    "    target_id: {} -> dest_grateid: {} -> in_grate_addr: {}",
                    target_id,
                    dest_grateid,
                    in_grate_addr
                );
            }
        }
    }
    lind_log!(THREEI, "====================");
}

/// Checks if a given cage has any registered syscall handlers in HANDLERTABLE.
///
/// ## Arguments:
/// - cageid: The ID of the cage to check.
///
/// ## Returns:
/// true if the cage has at least one handler registered.
/// false otherwise.
pub fn _check_cage_handler_exists(cageid: u64) -> bool {
    let handler_table = HANDLERTABLE.lock().unwrap();
    handler_table.contains_key(&cageid)
}

/// Lookup the interposed handler for a given (self_cageid, syscall_num, target_cageid).
///
/// 1. The lookup path is:
///        HANDLERTABLE[self_cageid][syscall_num][target_cageid]
///
/// 2. Dispatch is target-sensitive: the registered handler must match the
/// requested `target_cageid`. This avoids accidentally routing runtime-internal
/// calls to an unrelated grate registered for the same syscall number.
///
/// ## Arguments:
/// - `self_cageid`: The ID of the calling cage (the one executing the syscall).
/// - `syscall_num`: The number of the syscall being invoked.
/// - `target_cageid`: The ID of the target cage for the syscall.
///
/// ## Returns:
///     Some((handler_cageid, handler_addr)) if an exact mapping exists.
///     None if no handler entry exists for the given `target_cageid`.
///
/// ## Panics:
///     - If no entry exists for `self_cageid`.
///     - If no entry exists for `syscall_num`.
pub fn _get_handler(self_cageid: u64, syscall_num: u64, target_cageid: u64) -> Option<(u64, u64)> {
    let handler_table = HANDLERTABLE.lock().unwrap();

    let call_map = handler_table.get(&self_cageid).unwrap_or_else(|| {
        panic!(
            "[3i|_get_handler] no handler table for self_cageid: {}",
            self_cageid
        )
    });
    let target_map = call_map.get(&syscall_num).unwrap_or_else(|| {
        panic!(
            "[3i|_get_handler] no handler for syscall_num: {} in self_cageid: {}",
            syscall_num, self_cageid
        )
    });

    let (handler_cageid, addr) = target_map.get(&target_cageid)?;

    Some((*handler_cageid, *addr))
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
            target_map.retain(|_, (dest_grateid, _)| *dest_grateid != grateid);
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
/// See comments in threei.rs for details of high-level design.
///
/// ## Implementation details:
///
/// This function supports two behaviors according to the value of
/// `handlefunccage`.
///
/// Case 1: Remove handler for (srccage, targetcallnum)
///
/// If `handlefunccage` equals `THREEI_DEREGISTER`, the entire syscall entry
/// under `(srccage, targetcallnum)` is removed. This means that all registered
/// target cages for this syscall are cleared at once. After removal, the code
/// performs structural cleanup so that empty intermediate maps are deleted in
/// order to keep the table compact and avoid stale containers.
///
/// NOTE: If the caller intends to remove only a specific target cage for this syscall,
/// they must be sure they also register the desired handler for the target cage(s).
/// Otherwise, the upcoming syscall from the cage will cause a panic due to missing handler.
///
/// Case 2: Register or overwrite handler
///
/// In all other cases, the function performs registration or overwrite. The
/// `(srccage, targetcallnum)` containers are created if they do not already
/// exist. The handler is then inserted into the innermost map, replacing any
/// previous handler registered for the same lookup target. This keeps dispatch
/// target-sensitive: `make_syscall(self, callnum, target)` only considers the
/// entry registered for that exact `target`.
///
/// Because legacy glibc calls use `target_cageid == self_cageid`, RawPOSIX,
/// 3i-control, and grate interposition handlers use `srccage` as their lookup
/// key. Runtime callbacks that explicitly target Wasmtime use `WASMTIME_CAGEID`
/// as the lookup key. The value still records the true handler owner.
pub fn register_handler_impl(
    target_cageid: u64,
    srccage: u64,
    targetcallnum: u64,
    handlefunccage: u64,
    in_grate_fn_ptr_u64: u64,
) -> i32 {
    let mut table = HANDLERTABLE.lock().expect("HANDLERTABLE mutex poisoned");

    // Case 1: Remove syscall mapping for a given (srccage, targetcallnum)
    // If `handlefunccage == THREEI_DEREGISTER`, remove the entire callnum entry
    // for the given (targetcage, targetcallnum).
    if handlefunccage == threei_const::THREEI_DEREGISTER {
        if let Some(call_map) = table.get_mut(&srccage) {
            call_map.remove(&targetcallnum);

            if call_map.is_empty() {
                table.remove(&srccage);
            }
        }
        return 0;
    }

    // Case 2: Register or overwrite handler
    let call_map = table.entry(srccage).or_insert_with(HashMap::new);
    let target_map = call_map.entry(targetcallnum).or_insert_with(HashMap::new);
    let lookup_target = if target_cageid == lind_platform_const::WASMTIME_CAGEID {
        target_cageid
    } else {
        srccage
    };

    // Keep distinct target handlers side by side; only overwrite the same target.
    target_map.insert(lookup_target, (handlefunccage, in_grate_fn_ptr_u64));

    0
}

/// Actual implementation of copy_handler_table_to_cage.
/// See comments in threei.rs for details.
pub fn copy_handler_table_to_cage_impl(srccage: u64, targetcage: u64) -> u64 {
    let mut handler_table = HANDLERTABLE.lock().unwrap();

    // If srccage has a handler table, clones its contents into targetcage.
    // Overwrites any existing handlers in the target.
    if let Some(src_entry) = handler_table.get(&srccage).cloned() {
        handler_table.insert(targetcage, HashMap::new()); // overwrite whole target
        let target_entry = handler_table.get_mut(&targetcage).unwrap();
        for (callnum, callnum_map) in src_entry {
            let target_callnum_map = target_entry.entry(callnum).or_insert_with(HashMap::new);
            for (target_id, handler_entry) in callnum_map {
                // Self-targeted legacy entries must follow the copied cage.
                let copied_target_id = if target_id == srccage {
                    targetcage
                } else {
                    target_id
                };
                target_callnum_map
                    .entry(copied_target_id)
                    .or_insert(handler_entry);
            }
        }
        0
    } else {
        lind_log!(
            THREEI,
            "[3i|copy_handler_table_to_cage] srccage {} has no handler table",
            srccage
        );
        threei_const::ELINDAPIABORTED // treat missing src table as an error
    }
}
