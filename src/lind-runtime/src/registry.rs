//! Runtime registry: runtime ID → backend. The cage → runtime ID binding stays in 3i
//! (`threei::set_cage_runtime`), which Core consults before calling into a backend.

use crate::{LindError, LindResult, LindRuntime, RuntimeId};
use std::sync::{Arc, RwLock};
use sysdefs::constants::err_const::Errno;

static RUNTIMES: RwLock<Vec<Arc<dyn LindRuntime>>> = RwLock::new(Vec::new());

/// Register a backend, replacing any previous backend with the same runtime ID.
pub fn register_runtime(runtime: Arc<dyn LindRuntime>) {
    let mut runtimes = RUNTIMES.write().unwrap();
    let id = runtime.runtime_id();
    runtimes.retain(|existing| existing.runtime_id() != id);
    runtimes.push(runtime);
}

pub fn get_runtime(id: RuntimeId) -> Option<Arc<dyn LindRuntime>> {
    RUNTIMES
        .read()
        .unwrap()
        .iter()
        .find(|runtime| runtime.runtime_id() == id)
        .cloned()
}

pub fn registered_runtimes() -> Vec<Arc<dyn LindRuntime>> {
    RUNTIMES.read().unwrap().clone()
}

/// Select the first registered runtime whose `BelongsToRuntime` accepts `exe_path`.
/// Fails with the first probe error if every probe failed, otherwise with `ENOEXEC`.
pub fn runtime_for_executable(exe_path: &str) -> LindResult<Arc<dyn LindRuntime>> {
    let mut first_error = None;
    for runtime in registered_runtimes() {
        match runtime.belongs_to_runtime(exe_path) {
            Ok(true) => return Ok(runtime),
            Ok(false) => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    Err(first_error.unwrap_or(LindError::Other(Errno::ENOEXEC)))
}
