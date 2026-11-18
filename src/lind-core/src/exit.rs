//! exit / exit_group (Lind-IF §5 TerminateThread).

use crate::runtime::runtime_of;
use dashmap::DashMap;
use lind_runtime::{CageId, ThreadId};
use std::sync::atomic::Ordering;
use std::sync::LazyLock;

// `TerminateThread` carries no status, but a runtime reports the exiting thread's status as the
// guest entry's result (the launcher's root result). Core records it here for the runtime.
static THREAD_EXIT_STATUS: LazyLock<DashMap<(CageId, ThreadId), i32>> =
    LazyLock::new(DashMap::new);

/// Runtime → Core: the status recorded for an exiting thread.
pub fn take_thread_exit_status(cage: CageId, thread: ThreadId) -> Option<i32> {
    THREAD_EXIT_STATUS
        .remove(&(cage, thread))
        .map(|(_, status)| status)
}

fn terminate_self(cage: CageId, thread: ThreadId, status: i32) -> i64 {
    THREAD_EXIT_STATUS.insert((cage, thread), status);
    let result = runtime_of(cage).and_then(|runtime| runtime.terminate_thread(cage, thread));
    match result {
        Ok(()) => 0,
        Err(error) => {
            THREAD_EXIT_STATUS.remove(&(cage, thread));
            error.to_syscall_ret()
        }
    }
}

/// SYS_exit: terminate only the calling thread. The thread's status does not become the
/// cage's exit status (exit_group owns that).
pub fn exit_thread(cage: CageId, thread: ThreadId, status: i32) -> i64 {
    terminate_self(cage, thread, status)
}

/// SYS_exit_group: the first caller records the cage status and stops sibling threads; every
/// caller then terminates itself with the cage's canonical status.
pub fn exit_group(cage: CageId, thread: ThreadId, status: i32) -> i64 {
    if cage::signal::signal::try_initiate_exit_group(cage) {
        cage::cage_record_exit_status(cage, cage::ExitStatus::Exited(status));
        if let Some(c) = cage::get_cage(cage) {
            c.is_dead.store(true, Ordering::Release);
        }
        // Mark exiting before killing siblings so their late syscalls fail with ESRCH.
        threei::EXITING_TABLE.insert(cage);
        // TODO(Lind-IF §5): sibling teardown should go through TerminateThread for each sibling;
        // Lind-IF does not yet define remote (non-self) termination, so the Wasm epoch kill stays.
        cage::signal::signal::epoch_kill_all(cage, thread as i32);
        threei::handler_table::_rm_grate_from_handler(cage);
    }

    // Whichever thread won the exit_group race determines the status.
    let canonical = cage::get_cage(cage)
        .and_then(|c| *c.final_exit_status.read())
        .map(|status| match status {
            cage::ExitStatus::Exited(code) => code,
            cage::ExitStatus::Signaled(_, _) => 1,
        })
        .unwrap_or(status);
    terminate_self(cage, thread, canonical)
}
