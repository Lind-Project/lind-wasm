//! Unified syscall dispatch shims for clone, exec, and exit.
//!
//! A single 3i handler is registered per syscall.  Each shim resolves the
//! runtime type of the cage that originally issued the syscall, then
//! delegates to the appropriate runtime implementation via the `SyscallRuntime` trait.
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

use crate::lind_mpk::execute::MpkRuntime;
use crate::lind_wasmtime::execute::WasmtimeRuntime;

// ── Runtime trait ────────────────────────────────────────────────────────────

/// Trait for runtime-specific syscall handlers.
///
/// Each runtime (Wasmtime, MPK) implements this trait to provide its own
/// handling logic for clone, exec, and exit syscalls. The trait uses the
/// standard 3i calling convention: one cageid + six (arg, arg_cageid) pairs.
pub trait SyscallRuntime {
    /// Handle clone/fork syscall.
    fn handle_clone(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32;

    /// Handle exec syscall.
    fn handle_exec(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32;

    /// Handle exit syscall.
    fn handle_exit(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32;
}

// ── Runtime dispatch ─────────────────────────────────────────────────────────

/// Resolve a runtime type constant to its corresponding trait implementation.
fn get_runtime_handler(runtime_type: u64) -> &'static dyn SyscallRuntime {
    match runtime_type {
        threei_const::RUNTIME_TYPE_WASMTIME => &WasmtimeRuntime,
        threei_const::RUNTIME_TYPE_MPK => &MpkRuntime,
        _ => panic!("get_runtime_handler: unknown runtime_type={}", runtime_type),
    }
}

// ── Syscall shims ────────────────────────────────────────────────────────────

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
    let runtime_type = threei::get_cage_runtime(parent_cageid)
        .unwrap_or_else(|| {
            panic!(
                "shim_clone_handler: no runtime found for parent_cageid={}",
                parent_cageid
            )
        });

    let runtime = get_runtime_handler(runtime_type);
    runtime.handle_clone(
        cageid,
        arg1, arg1_cageid,
        arg2, arg2_cageid,
        arg3, arg3_cageid,
        arg4, arg4_cageid,
        arg5, arg5_cageid,
        arg6, arg6_cageid,
    )
}

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
    let runtime_type = threei::get_cage_runtime(execing_cageid)
        .unwrap_or_else(|| {
            panic!(
                "shim_exec_handler: no runtime found for execing_cageid={}",
                execing_cageid
            )
        });

    let runtime = get_runtime_handler(runtime_type);
    runtime.handle_exec(
        cageid,
        arg1, arg1_cageid,
        arg2, arg2_cageid,
        arg3, arg3_cageid,
        arg4, arg4_cageid,
        arg5, arg5_cageid,
        arg6, arg6_cageid,
    )
}

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
    let runtime_type = threei::get_cage_runtime(exiting_cageid)
        .unwrap_or_else(|| {
            panic!(
                "shim_exit_handler: no runtime found for exiting_cageid={}",
                exiting_cageid
            )
        });

    let runtime = get_runtime_handler(runtime_type);
    runtime.handle_exit(
        cageid,
        arg1, arg1_cageid,
        arg2, arg2_cageid,
        arg3, arg3_cageid,
        arg4, arg4_cageid,
        arg5, arg5_cageid,
        arg6, arg6_cageid,
    )
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
