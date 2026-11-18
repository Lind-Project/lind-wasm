//! Core-side access to the runtime bound to a cage, and the initial launch transaction.

use lind_runtime::{
    CageId, LindCloneArgs, LindError, LindInitArgs, LindResult, LindRuntime, RuntimeId, ThreadId,
};
use std::sync::Arc;
use sysdefs::constants::err_const::Errno;

/// Guest thread ID of a cage's initial thread.
pub const MAIN_THREAD_ID: ThreadId = 1;

/// The runtime responsible for `cage`, resolved through the 3i cage → runtime binding.
pub fn runtime_of(cage: CageId) -> LindResult<Arc<dyn LindRuntime>> {
    let id = threei::get_cage_runtime(cage).ok_or(LindError::Other(Errno::ESRCH))?;
    lind_runtime::get_runtime(id).ok_or(LindError::Other(Errno::ENOSYS))
}

/// Initial launch (Lind-IF §4/§5): the cage record already exists; bind it to `runtime`,
/// initialize its image and create its initial thread. Execution may start before this
/// returns; completion is observed through `LindCageManager`, not through this call.
pub fn launch_cage(
    cage: CageId,
    runtime: RuntimeId,
    exe_path: &str,
    init_args: LindInitArgs,
) -> LindResult<()> {
    threei::set_cage_runtime(cage, runtime);
    let backend = match runtime_of(cage) {
        Ok(backend) => backend,
        Err(error) => {
            threei::remove_cage_runtime(cage);
            return Err(error);
        }
    };

    let created = backend
        .init_cage(cage, exe_path, init_args, false)
        .and_then(|()| backend.create_thread(cage, MAIN_THREAD_ID, LindCloneArgs::default()));
    if let Err(error) = created {
        // `CreateThread` failures leave no runnable thread; release whatever InitCage set up.
        if let Err(cleanup) = backend.terminate_cage(cage) {
            eprintln!("[lind-core] failed to release cage {cage} after launch error: {cleanup:?}");
        }
        threei::remove_cage_runtime(cage);
        return Err(error);
    }
    Ok(())
}
