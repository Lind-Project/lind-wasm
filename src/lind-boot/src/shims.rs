//! Unified syscall dispatch shims for clone, exec, and exit.
//!
//! A single 3i handler is registered per syscall.  Each shim resolves the
//! runtime type of the cage that originally issued the syscall, then
//! delegates to the Wasmtime or MPK backend handler.
//!
//! Argument layout when a shim is invoked (matches `threei::RawCallFunc`):
//!
//!   clone  (CLONE3_SYSCALL = 435), dispatched by RawPOSIX `fork_syscall`:
//!     cageid          = WASMTIME_CAGEID (handler cage, not the calling cage)
//!     arg1            = clone_arg (ptr to clone_args struct)
//!     arg1_cageid     = clone_arg_cageid
//!     arg2            = parent_cageid  ← runtime is looked up here
//!     arg2_cageid     = parent_tid
//!     arg3            = child_cageid
//!     …
//!
//!   exec   (EXEC_SYSCALL = 59), dispatched by RawPOSIX `exec_syscall`:
//!     cageid          = WASMTIME_CAGEID
//!     arg1            = path
//!     arg1_cageid     = execing_cageid ← runtime is looked up here
//!     arg2            = argv
//!     …
//!
//!   exit   (EXIT_SYSCALL = 60), dispatched by RawPOSIX `exit_syscall`:
//!     cageid          = WASMTIME_CAGEID
//!     arg1            = exit_status
//!     arg1_cageid     = exiting_cageid ← runtime is looked up here
//!     arg2            = tid
//!     …

use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, UNUSED_ARG, UNUSED_ID, WASMTIME_CAGEID};
use sysdefs::constants::syscall_const::{CLONE3_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL};
use threei::threei_const;

use crate::lind_mpk::syscalls::{mpk_clone_syscall_entry, mpk_exit_syscall_entry};
use crate::lind_wasmtime::trampoline::{clone_syscall_entry, exec_syscall_entry, exit_syscall_entry};

// ── clone shim ───────────────────────────────────────────────────────────────

extern "C" fn shim_clone_handler(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,         // parent_cageid — whose runtime we dispatch on
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let parent_cageid = arg2;
    match threei::get_cage_runtime(parent_cageid) {
        Some(rt) if rt == threei_const::RUNTIME_TYPE_WASMTIME => clone_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        ),
        Some(rt) if rt == threei_const::RUNTIME_TYPE_MPK => mpk_clone_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        ),
        other => panic!(
            "shim_clone_handler: unrecognised runtime {:?} for parent_cageid={}",
            other, parent_cageid
        ),
    }
}

// ── exec shim ────────────────────────────────────────────────────────────────

extern "C" fn shim_exec_handler(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,  // execing_cageid — whose runtime we dispatch on
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let execing_cageid = arg1_cageid;
    match threei::get_cage_runtime(execing_cageid) {
        Some(rt) if rt == threei_const::RUNTIME_TYPE_WASMTIME => exec_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        ),
        Some(rt) if rt == threei_const::RUNTIME_TYPE_MPK => {
            // MPK exec not yet implemented.
            todo!("shim_exec_handler: MPK exec unimplemented")
        }
        other => panic!(
            "shim_exec_handler: unrecognised runtime {:?} for execing_cageid={}",
            other, execing_cageid
        ),
    }
}

// ── exit shim ────────────────────────────────────────────────────────────────

extern "C" fn shim_exit_handler(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,  // exiting_cageid — whose runtime we dispatch on
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let exiting_cageid = arg1_cageid;
    match threei::get_cage_runtime(exiting_cageid) {
        Some(rt) if rt == threei_const::RUNTIME_TYPE_WASMTIME => exit_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        ),
        Some(rt) if rt == threei_const::RUNTIME_TYPE_MPK => mpk_exit_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        ),
        other => panic!(
            "shim_exit_handler: unrecognised runtime {:?} for exiting_cageid={}",
            other, exiting_cageid
        ),
    }
}

// ── registration ─────────────────────────────────────────────────────────────

/// Register the three unified syscall shims with the 3i handler table.
///
/// Must be called once at boot time, after `rawposix_start()` and before any
/// cage is created.  Replaces the separate `register_wasmtime_syscall_entry()`
/// and `register_mpk_syscall_entry()` calls that previously lived in each
/// backend's `execute.rs`.
pub fn register_syscall_entries() {
    let clone_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,
        RAWPOSIX_CAGEID,
        CLONE3_SYSCALL as u64,
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime_id: required by the API, unused by handler_table
        WASMTIME_CAGEID,
        shim_clone_handler as *const () as u64,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID,
    );

    let exec_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,
        RAWPOSIX_CAGEID,
        EXEC_SYSCALL as u64,
        threei_const::RUNTIME_TYPE_WASMTIME,
        WASMTIME_CAGEID,
        shim_exec_handler as *const () as u64,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID,
    );

    let exit_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,
        RAWPOSIX_CAGEID,
        EXIT_SYSCALL as u64,
        threei_const::RUNTIME_TYPE_WASMTIME,
        WASMTIME_CAGEID,
        shim_exit_handler as *const () as u64,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID,
    );

    assert!(
        clone_ret == 0 && exec_ret == 0 && exit_ret == 0,
        "[lind-boot] register_syscall_entries failed: clone={} exec={} exit={}",
        clone_ret, exec_ret, exit_ret
    );
}
