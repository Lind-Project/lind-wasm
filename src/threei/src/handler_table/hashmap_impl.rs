use crate::threei_const;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::Mutex;
use sysdefs::constants::lind_platform_const;

/// HANDLERTABLE:
/// A nested hash map used to define fine-grained per-syscall interposition rules.
///
/// <self_cageid, <callnum, (dest_grateid, in_grate_addr)>
/// Keys are the grate, the value is a HashMap with a key of the callnum
/// and the values are a (dest_grateid, in_grate_addr) tuple for the actual handlers...
type TargetCageMap = HashMap<u64, u64>; // Maps dest_grateid to destfunc in grate addr
type CallnumMap = HashMap<u64, TargetCageMap>; // Maps targetcallnum to TargetCageMap
type CageHandlerTable = HashMap<u64, CallnumMap>; // Maps self_cageid to CallnumMap

lazy_static::lazy_static! {
    // <self_cageid, <callnum, (dest_grateid, in_grate_addr)>
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
            for (dest_grateid, in_grate_addr) in target_map.iter() {
                println!(
                    "    dest_grateid: {} -> in_grate_addr: {}",
                    dest_grateid, in_grate_addr
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
pub fn _check_cage_handler_exists(cageid: u64) -> bool {
    let handler_table = HANDLERTABLE.lock().unwrap();
    handler_table.contains_key(&cageid)
}

/// Lookup the interposed handler for a given (self_cageid, syscall_num, target_cageid).
///
/// 1. The lookup path is:
///        HANDLERTABLE[self_cageid][syscall_num][target_cageid]
///
/// 2. If `target_cageid == self_cageid` (i.e., RAWPOSIX lookup):
///        - If an explicit RAWPOSIX handler exists, return it.
///        - Otherwise, fallback to ANY registered handler under
///          (self_cageid, syscall_num).
///          This allows RAWPOSIX to behave as a default dispatch target
///          when no explicit RAWPOSIX entry was installed.
///    Note: theoretically there could be only one **grate** handlers for each cage
///           Execeptions should only happen for fork/exec/exit calls (having WASMTIME_CAGEID entries)
///
/// 3. If `target_cageid != self_cageid`:
///        - An exact match is REQUIRED.
///        - If not found, panic (this is considered a logic error).
///
/// ## Arguments:
/// - `self_cageid`: The ID of the calling cage (the one executing the syscall).
/// - `syscall_num`: The number of the syscall being invoked.
/// - `target_cageid`: The ID of the target cage for the syscall.
///
/// ## Returns:
///     Some((actual_target_cageid, handler_addr))
///
/// ## Panics:
///     - If no entry exists for `self_cageid`.
///     - If no entry exists for `syscall_num`.
///     - If non-RAWPOSIX lookup misses.
pub fn _get_handler(self_cageid: u64, syscall_num: u64, target_cageid: u64) -> Option<(u64, u64)> {
    let handler_table = HANDLERTABLE.lock().unwrap();

    // self_cageid -> callnum map
    let call_map = handler_table.get(&self_cageid).unwrap_or_else(|| {
        panic!(
            "No handler table for self_cageid={} (syscall_num={}, dest_grateid={})",
            self_cageid, syscall_num, target_cageid
        )
    });

    // callnum -> target map
    let target_map = call_map.get(&syscall_num).unwrap_or_else(|| {
        panic!(
            "No handlers for self_cageid={} syscall_num={} (dest_grateid={})",
            self_cageid, syscall_num, target_cageid
        )
    });

    // When target_cageid == self_cageid, this means the call is supposed to be handled
    // within the same cage or this is a cage call, so we first check if a RAWPOSIX handler exists.
    // More details on scenarios and the reason we use this check in the comments
    // in `register_handler_impl`.
    // If it does, we return it. If not, we fallback to any handler registered for this
    // (self_cageid, syscall_num) pair,
    if target_cageid == self_cageid {
        // Prefer exact RAWPOSIX or THREEI handler if registered
        if let Some(addr) = target_map.get(&lind_platform_const::RAWPOSIX_CAGEID) {
            return Some((lind_platform_const::RAWPOSIX_CAGEID, *addr));
        } else if let Some(addr) = target_map.get(&lind_platform_const::THREEI_CAGEID) {
            return Some((lind_platform_const::THREEI_CAGEID, *addr));
        }

        let grateid = target_map.keys().next().copied()?;
        let addr = target_map.values().next().copied()?;
        // Otherwise fallback to any registered handler
        return Some((grateid, addr));
    }

    // Non-RAWPOSIX: exact match required
    match target_map.get(&target_cageid) {
        Some(addr) => Some((target_cageid, *addr)),
        None => panic!(
            "Handler not found for (self_cageid={}, syscall_num={}, target_cageid={})",
            self_cageid, syscall_num, target_cageid
        ),
    }
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
            target_map.retain(|dest_grateid, _| *dest_grateid != grateid);
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
/// This function supports three distinct behaviors according to the value of
/// `handlefunccage` and `register_flag`.
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
/// previous handler registered for the same `handlefunccage`. If a RAWPOSIX
/// fallback handler exists under the same syscall, it is removed before the
/// new handler is inserted. This ensures that RAWPOSIX behaves strictly as a
/// fallback dispatch target and does not shadow a more specific interposed
/// grate handler.
///
/// At the moment, all glibc-originated cage syscalls issued through
/// `MAKE_LEGACY_SYSCALL` unconditionally set the target cage ID to
/// `self_cageid`. From the perspective of the glibc cage, every
/// syscall is therefore dispatched toward RAWPOSIX by default. This
/// means that even if a grate has already registered an interposed
/// handler for a given `(srccage, syscall)`, the cage itself has no
/// prior knowledge of that registration. The grate knows, and 3i knows,
/// but the cage does not.
///
/// As a result, when the syscall reaches 3i, distinguishing the true
/// intended target becomes difficult. The original target cage was set
/// to self_cageid by glibc, and no additional metadata is available at the
/// call site to indicate that a non-RAWPOSIX grate handler should be
/// preferred. One possible strategy would be to infer intent based on
/// the number of registered handlers, for example choosing a non-RAWPOSIX
/// handler whenever more than one entry exists.
///
/// To reduce complexity and avoid ambiguous runtime inference, we adopt
/// a simpler registration policy. Whenever a specific grate handler is
/// registered for a `(srccage, syscall)` pair and a RAWPOSIX entry already
/// exists, the RAWPOSIX entry is removed and replaced. This ensures that
/// RAWPOSIX remains strictly a fallback target and cannot coexist in a
/// misleading way with a more specific handler. By enforcing this rule
/// at registration time, we eliminate the need for complicated dispatch
/// disambiguation logic later in 3i and keep the runtime decision path
/// deterministic and structurally clean.
pub fn register_handler_impl(
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

    // If a RAWPOSIX/3i fallback handler exists for this (srccage, targetcallnum),
    // remove it to ensure it does not shadow the new handler.
    if target_map.contains_key(&lind_platform_const::RAWPOSIX_CAGEID) {
        target_map.remove(&lind_platform_const::RAWPOSIX_CAGEID);
    }
    if target_map.contains_key(&lind_platform_const::THREEI_CAGEID) {
        target_map.remove(&lind_platform_const::THREEI_CAGEID);
    }

    target_map.insert(handlefunccage, in_grate_fn_ptr_u64);

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
