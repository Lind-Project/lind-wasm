//! clone/fork transaction (Lind-IF §4 DuplicateCage + §5 DuplicateThread).

use crate::fork::prepare_fork;
use crate::runtime::{runtime_of, MAIN_THREAD_ID};
use lind_runtime::{
    CageId, LindThreadDuplicateArgs, ThreadDuplicateMode, ThreadId, UserPtr,
    RUNTIME_ASSIGNED_THREAD,
};

/// fork-like clone (no `CLONE_VM`). Returns the child cage ID or a negative errno.
///
/// The Wasm backend completes the child after the parent's Asyncify unwind, so a
/// runtime failure after this returns is rolled back through `fork::abort_unstarted_fork`.
pub fn clone_cage(parent: CageId, parent_thread: ThreadId, clone_args: UserPtr) -> i64 {
    let runtime = match runtime_of(parent) {
        Ok(runtime) => runtime,
        Err(error) => return error.to_syscall_ret(),
    };
    // TODO(Lind-IF §4): stop/capture the selected thread when forking on behalf of an
    // independently running target; ordinary fork runs on the selected thread itself.
    let pending = match prepare_fork(parent) {
        Ok(pending) => pending,
        Err(errno) => return errno,
    };
    let child = pending.cageid();

    if let Err(error) = runtime.duplicate_cage(parent, child) {
        return error.to_syscall_ret();
    }
    let args = LindThreadDuplicateArgs {
        mode: ThreadDuplicateMode::SyscallFork,
        clone_args,
    };
    match runtime.duplicate_thread(parent, parent_thread, child, MAIN_THREAD_ID, args) {
        Ok(()) => {
            pending.commit();
            child as i64
        }
        Err(error) => {
            let _ = runtime.terminate_cage(child);
            error.to_syscall_ret()
        }
    }
}

/// Thread clone (`CLONE_VM`, the libc pthread ABI): `DuplicateThread` without `DuplicateCage`.
pub fn clone_thread(cage: CageId, parent_thread: ThreadId, clone_args: UserPtr) -> i64 {
    let runtime = match runtime_of(cage) {
        Ok(runtime) => runtime,
        Err(error) => return error.to_syscall_ret(),
    };
    let args = LindThreadDuplicateArgs {
        mode: ThreadDuplicateMode::PthreadClone,
        clone_args,
    };
    match runtime.duplicate_thread(cage, parent_thread, cage, RUNTIME_ASSIGNED_THREAD, args) {
        Ok(()) => 0,
        Err(error) => error.to_syscall_ret(),
    }
}
