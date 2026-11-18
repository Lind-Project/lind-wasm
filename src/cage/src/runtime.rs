//! Registry of Lind-IF runtimes, keyed by `RUNTIME_TYPE_*`.
//!
//! Runtimes register themselves at boot so Core can call into them (e.g. to
//! notify a signal) without depending on their implementations.

use crate::cage::get_cage;
use lind_if::LindRuntime;
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
