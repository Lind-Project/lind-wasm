//! Registry of Lind-IF runtimes, keyed by `RUNTIME_TYPE_*`.
//!
//! Runtimes register themselves at boot so Core can call into them (e.g. to
//! notify a signal) without depending on their implementations.

use crate::cage::get_cage;
use lind_if::{CageId, LindCore, LindRuntime, ThreadId};
use std::sync::atomic::Ordering;
use std::sync::RwLock;

static RUNTIMES: RwLock<Vec<(u64, &'static dyn LindRuntime)>> = RwLock::new(Vec::new());

/// Registers `runtime` for `runtime_type`, replacing any previous registration.
pub fn register_runtime(runtime_type: u64, runtime: &'static dyn LindRuntime) {
    let mut runtimes = RUNTIMES.write().unwrap();
    match runtimes.iter_mut().find(|(ty, _)| *ty == runtime_type) {
        Some(entry) => entry.1 = runtime,
        None => runtimes.push((runtime_type, runtime)),
    }
}

pub fn registered_runtime(runtime_type: u64) -> Option<&'static dyn LindRuntime> {
    RUNTIMES
        .read()
        .unwrap()
        .iter()
        .find(|(ty, _)| *ty == runtime_type)
        .map(|(_, runtime)| *runtime)
}

/// Returns the registered runtime currently executing `cageid`.
pub fn cage_runtime(cageid: u64) -> Option<&'static dyn LindRuntime> {
    let cage = get_cage(cageid)?;
    registered_runtime(cage.runtime_type.load(Ordering::Acquire))
}

/// Core's Lind-IF services (section 8). Boot registers [`CORE`] with
/// `lind_if::register_core`.
pub struct CageCore;

pub static CORE: CageCore = CageCore;

impl LindCore for CageCore {
    /// Completes a thread exit (Lind-IF section 5, Completion and teardown).
    fn thread_exited(&self, cage_id: CageId, thread_id: ThreadId) -> bool {
        let is_last = crate::signal::lind_thread_exit(cage_id, thread_id as u64);
        if is_last {
            if let Some(cage) = get_cage(cage_id) {
                while cage.grate_inflight.load(Ordering::Acquire) > 0 {
                    std::hint::spin_loop();
                }
            }
            if let Some(runtime) = cage_runtime(cage_id) {
                // The cage is finalized regardless; a runtime that cannot
                // release its metadata leaks it.
                let _ = runtime.terminate_cage(cage_id);
            }
            crate::cage::cage_finalize(cage_id);
        }
        is_last
    }
}
