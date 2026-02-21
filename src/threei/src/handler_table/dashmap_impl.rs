use crate::threei_const;
use dashmap::DashMap;
use std::sync::Mutex;
use sysdefs::constants::lind_platform_const;

/// HANDLERTABLE:
/// A nested hash map used to define fine-grained per-syscall interposition rules.
///
/// <self_cageid, <callnum, (dest_grateid, in_grate_addr)>
/// Keys are the grate, the value is a HashMap with a key of the callnum
/// and the values are a (dest_grateid, in_grate_addr) tuple for the actual handlers...
type TargetCageMap = DashMap<u64, u64>; // Maps dest_grateid to destfunc in grate addr
type CallnumMap = DashMap<u64, TargetCageMap>; // Maps targetcallnum to TargetCageMap
type CageHandlerTable = DashMap<u64, CallnumMap>; // Maps self_cageid to CallnumMap

lazy_static::lazy_static! {
    // <self_cageid, <callnum, (dest_grateid, in_grate_addr)>>
    // callnum is mapped to addr, not self
    pub static ref HANDLERTABLE: CageHandlerTable = DashMap::new();
}

/// Helper function for debugging.
/// Prints the current contents of `HANDLERTABLE` in a readable format
/// to help inspect cage–callnum–target mappings during development.
pub fn print_handler_table() {
    println!("=== HANDLERTABLE ===");
    for self_entry in HANDLERTABLE.iter() {
        let self_cageid = *self_entry.key();
        println!("CageID: {}", self_cageid);

        let callnum_map: &CallnumMap = self_entry.value();
        for call_entry in callnum_map.iter() {
            let callnum = *call_entry.key();
            println!("  Callnum: {}", callnum);

            let target_map: &TargetCageMap = call_entry.value();
            for target_entry in target_map.iter() {
                println!(
                    "    dest_grateid: {} -> in_grate_addr: {}",
                    *target_entry.key(),
                    *target_entry.value()
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
    HANDLERTABLE.contains_key(&cageid)
}

/// Lookup the interposed handler for a given (self_cageid, syscall_num, target_cageid).
///
/// 1. The lookup path is:
///        HANDLERTABLE[self_cageid][syscall_num][target_cageid]
///
/// 2. If `target_cageid == RAWPOSIX_CAGEID`:
///        - If an explicit RAWPOSIX handler exists, return it.
///        - Otherwise, fallback to ANY registered handler under
///          (self_cageid, syscall_num).
///          This allows RAWPOSIX to behave as a default dispatch target
///          when no explicit RAWPOSIX entry was installed.
///    Note: theoretically there could be only one **grate** handlers for each cage
///           Execeptions should only happen for fork/exec/exit calls (having WASMTIME_CAGEID entries)
///
/// 3. If `target_cageid != RAWPOSIX_CAGEID`:
///        - An exact match is REQUIRED.
///        - If not found, panic (this is considered a logic error).
///
/// ## Arguments:
/// - self_cageid: The ID of the calling cage (the one executing the syscall).
/// - syscall_num: The number of the syscall being invoked.
/// - target_cageid: The ID of the target cage for the syscall.
///
/// ## Returns:
///     Some((target_cageid, addr)) if a handler is found according to the lookup rules.
///
/// ## Panics:
///     - If no entry exists for self_cageid.
///     - If no entry exists for syscall_num.
///     - If non-RAWPOSIX lookup misses.
pub fn _get_handler(self_cageid: u64, syscall_num: u64, target_cageid: u64) -> Option<(u64, u64)> {
    // Grab the per-cage map guard.
    let self_entry = HANDLERTABLE.get(&self_cageid).unwrap_or_else(|| {
        panic!(
            "No handler table for self_cageid={} (syscall_num={}, target_cageid={})",
            self_cageid, syscall_num, target_cageid
        )
    });

    // Grab the per-syscall map guard.
    let call_entry = self_entry.value().get(&syscall_num).unwrap_or_else(|| {
        panic!(
            "No handlers for self_cageid={} syscall_num={} (target_cageid={})",
            self_cageid, syscall_num, target_cageid
        )
    });

    let target_map = call_entry.value();

    // RAWPOSIX special case: prefer explicit RAWPOSIX handler; otherwise fallback to any.
    if target_cageid == lind_platform_const::RAWPOSIX_CAGEID {
        if let Some(addr_ref) = target_map.get(&lind_platform_const::RAWPOSIX_CAGEID) {
            let addr = *addr_ref.value();
            return Some((lind_platform_const::RAWPOSIX_CAGEID, addr));
        }

        // Best-effort fallback: pick any registered handler.
        if let Some(any) = target_map.iter().next() {
            let gid = *any.key();
            let addr = *any.value();
            return Some((gid, addr));
        }
        return None;
    }

    // Non-RAWPOSIX requires exact match.
    if let Some(addr_ref) = target_map.get(&target_cageid) {
        let addr = *addr_ref.value();
        return Some((target_cageid, addr));
    }

    panic!(
        "Handler not found for (self_cageid={}, syscall_num={}, target_cageid={})",
        self_cageid, syscall_num, target_cageid
    );
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
    for self_entry in HANDLERTABLE.iter() {
        let call_map: &CallnumMap = self_entry.value();
        for call_entry in call_map.iter() {
            let target_map: &TargetCageMap = call_entry.value();
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
    HANDLERTABLE.remove(&cageid);
}

/// Actual implementation of register_handler.
/// See comments in threei.rs for details of high-level design.
///
/// ## Implementation details:
///
/// This function supports three distinct behaviors according to the value of
/// `handlefunccage` and `register_flag`.
///
/// Case 1: Remove ALL handlers for (srccage, targetcallnum)
///
/// If `handlefunccage` equals `THREEI_DEREGISTER`, the entire syscall entry
/// under `(srccage, targetcallnum)` is removed. This means that all registered
/// target cages for this syscall are cleared at once. After removal, the code
/// performs structural cleanup so that empty intermediate maps are deleted in
/// order to keep the table compact and avoid stale containers.
///
/// Case 2: Remove ONLY a specific handler entry
///
/// If `register_flag` equals zero, only the specific entry
/// `(srccage, targetcallnum, handlefunccage)` is removed. In this mode,
/// deregistration is granular and does not affect other target cages registered
/// for the same syscall. After removing the specific entry, the function checks
/// whether the inner maps have become empty and removes them accordingly,
/// maintaining structural consistency of the nested hash map.
///
/// Case 3: Register or overwrite handler
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
/// `RAWPOSIX_CAGEID`. From the perspective of the glibc cage, every
/// syscall is therefore dispatched toward RAWPOSIX by default. This
/// means that even if a grate has already registered an interposed
/// handler for a given `(srccage, syscall)`, the cage itself has no
/// prior knowledge of that registration. The grate knows, and 3i knows,
/// but the cage does not.
///
/// As a result, when the syscall reaches 3i, distinguishing the true
/// intended target becomes difficult. The original target cage was set
/// to RAWPOSIX by glibc, and no additional metadata is available at the
/// call site to indicate that a non-RAWPOSIX grate handler should be
/// preferred. One possible strategy would be to infer intent based on
/// the number of registered handlers, for example choosing a non-RAWPOSIX
/// handler whenever more than one entry exists. However, this heuristic
/// breaks down for syscalls such as `clone`, `exec`, and `exit`, which
/// inherently require multiple handlers (e.g., one for RAWPOSIX and one
/// for Wasmtime). In those cases, handler multiplicity does not encode
/// meaningful dispatch intent.
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
    register_flag: u64, // 0: deregister, otherwise: register
    handlefunccage: u64,
    in_grate_fn_ptr_u64: u64,
) -> i32 {
    // Case 1: remove all handlers for (srccage, targetcallnum)
    if handlefunccage == threei_const::THREEI_DEREGISTER {
        if let Some(self_entry) = HANDLERTABLE.get(&srccage) {
            let call_map: &CallnumMap = self_entry.value();
            call_map.remove(&targetcallnum);

            // If the per-cage map is now empty, remove the cage entry.
            if call_map.is_empty() {
                drop(self_entry);
                HANDLERTABLE.remove(&srccage);
            }
        }
        return 0;
    }
    // Case 2: remove only (srccage, targetcallnum, handlefunccage)
    if register_flag == 0 {
        if let Some(self_entry) = HANDLERTABLE.get(&srccage) {
            let call_map: &CallnumMap = self_entry.value();

            if let Some(call_entry) = call_map.get(&targetcallnum) {
                let target_map: &TargetCageMap = call_entry.value();
                target_map.remove(&handlefunccage);

                // If no handlers remain for this syscall, remove the callnum entry.
                if target_map.is_empty() {
                    drop(call_entry);
                    call_map.remove(&targetcallnum);
                }
            }

            // If no syscalls remain for this cage, remove the cage entry.
            if call_map.is_empty() {
                drop(self_entry);
                HANDLERTABLE.remove(&srccage);
            }
        }
        return 0;
    }

    // Case 3: register or overwrite handler
    let call_map_ref = HANDLERTABLE.entry(srccage).or_insert_with(DashMap::new);
    let call_map: &CallnumMap = &*call_map_ref;

    let target_map_ref = call_map.entry(targetcallnum).or_insert_with(DashMap::new);
    let target_map: &TargetCageMap = &*target_map_ref;

    // If RAWPOSIX fallback exists, remove it so it does not shadow the new handler.
    target_map.remove(&lind_platform_const::RAWPOSIX_CAGEID);

    target_map.insert(handlefunccage, in_grate_fn_ptr_u64);

    0
}

/// Actual implementation of copy_handler_table_to_cage.
/// See comments in threei.rs for details.
pub fn copy_handler_table_to_cage_impl(srccage: u64, targetcage: u64) -> u64 {
    // Clone the source call map snapshot so we don't hold a long-lived reference
    // while mutating the destination. DashMap::clone clones the structure and keys/values.
    let src_snapshot: CallnumMap = if let Some(src_entry) = HANDLERTABLE.get(&srccage) {
        let snap = src_entry.value().clone();
        drop(src_entry);
        snap
    } else {
        eprintln!(
            "[3i|copy_handler_table_to_cage] srccage {} has no handler table",
            srccage
        );
        return threei_const::ELINDAPIABORTED;
    };

    let dst_call_map_ref = HANDLERTABLE.entry(targetcage).or_insert_with(DashMap::new);
    let dst_call_map: &CallnumMap = &*dst_call_map_ref;

    // Copy without overwriting existing destination handlers.
    for src_call_entry in src_snapshot.iter() {
        let callnum = *src_call_entry.key();
        let src_target_map: &TargetCageMap = src_call_entry.value();

        let dst_target_map_ref = dst_call_map.entry(callnum).or_insert_with(DashMap::new);
        let dst_target_map: &TargetCageMap = &*dst_target_map_ref;

        for src_target_entry in src_target_map.iter() {
            let handlefunccage = *src_target_entry.key();
            let addr = *src_target_entry.value();

            // Insert only if absent, preserving any existing destination mapping.
            dst_target_map.entry(handlefunccage).or_insert(addr);
        }
    }

    0
}
