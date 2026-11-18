//! exec transaction (Lind-IF §4 InitCage(staging) → CreateThread → ApplyStagingCage).

use crate::runtime::{runtime_of, MAIN_THREAD_ID};
use lind_runtime::{
    CageId, LindArgVector, LindCloneArgs, LindError, LindInitArgs, LindResult, LindRuntime,
    UserPtr,
};
use std::sync::atomic::{AtomicU64, Ordering};
use sysdefs::constants::err_const::Errno;

/// Staging identities live outside the cage ID space so they are never waitable or routable.
pub const STAGING_ID_FLAG: CageId = 1 << 63;
static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

pub fn is_staging_id(id: CageId) -> bool {
    id & STAGING_ID_FLAG != 0
}

fn allocate_staging_id() -> CageId {
    STAGING_ID_FLAG | NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed)
}

/// Replace the program of `cage`. Returns 0 once the replacement is committed (the old guest
/// continuation never resumes) or a negative errno while the old program is still intact.
pub fn exec(cage: CageId, path: UserPtr, argv: UserPtr, envp: UserPtr) -> i64 {
    match exec_inner(cage, path, argv, envp) {
        Ok(()) => 0,
        Err(error) => error.to_syscall_ret(),
    }
}

fn exec_inner(cage: CageId, path: UserPtr, argv: UserPtr, envp: UserPtr) -> LindResult<()> {
    // Pointers reaching Core are already host addresses (see Lind-IF §8 gap).
    let exe_path =
        typemap::get_cstr_lossy(path.addr).map_err(|_| LindError::Other(Errno::EFAULT))?;
    let current = runtime_of(cage)?;
    let target = lind_runtime::runtime_for_executable(&exe_path)?;
    if target.runtime_id() != current.runtime_id() {
        // Cross-runtime exec needs two-provider coordination that is not implemented yet.
        return Err(LindError::Other(Errno::ENOSYS));
    }

    let staging = allocate_staging_id();
    let init_args = LindInitArgs {
        argv: LindArgVector::User(argv),
        envp: LindArgVector::User(envp),
    };
    let prepared = target
        .init_cage(staging, &exe_path, init_args, true)
        .and_then(|()| target.create_thread(staging, MAIN_THREAD_ID, LindCloneArgs::default()))
        .and_then(|()| check_exec_preserves_handlers(cage));
    if let Err(error) = prepared {
        discard_staging(target.as_ref(), staging);
        return Err(error);
    }

    if let Err(error) = target.apply_staging_cage(staging, cage) {
        // The Wasm backend fails only before its point of no return; see Lind-IF gap on
        // distinguishing pre-/post-commit apply failures.
        discard_staging(target.as_ref(), staging);
        return Err(error);
    }
    apply_exec_resource_rules(cage);
    Ok(())
}

fn discard_staging(runtime: &dyn LindRuntime, staging: CageId) {
    if let Err(error) = runtime.terminate_cage(staging) {
        eprintln!("[lind-core] failed to discard exec staging {staging:#x}: {error:?}");
    }
}

/// Lind-IF §4: exec must be rejected (proposed `EBUSY`) if it would invalidate a registered
/// handler entry. 3i does not yet expose incoming registrations per cage, so nothing is checked.
fn check_exec_preserves_handlers(_cage: CageId) -> LindResult<()> {
    Ok(())
}

/// Linux exec resource rules owned by Core: close-on-exec descriptors, shared memory
/// attachments, mappings, signal dispositions and mask.
fn apply_exec_resource_rules(cage: CageId) {
    fdtables::empty_fds_for_exec(cage);
    let Some(selfcage) = cage::get_cage(cage) else {
        return;
    };
    cage::unmap_shm_mappings(cage);
    selfcage.vmmap.write().clear();
    // Dispositions reset to default; pending signals are preserved.
    selfcage.signalhandler.clear();
    selfcage.sigset.store(0, Ordering::Relaxed);
    // epoch_handler/main_threadid are kept: the new instance overwrites them during
    // lind_signal_init, and a crashing replacement still needs them for exit tracking.
}
