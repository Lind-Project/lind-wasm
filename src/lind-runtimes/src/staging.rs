//! Staged exec images shared by the runtimes' `init_cage`/`apply_staging_cage`.
//! Staging is private to a runtime: Core only names the staging cage for the
//! duration of one exec.

use lind_if::{CageId, Errno, LindError, LindResult, ThreadId};
use std::sync::Mutex;

/// An image prepared by `init_cage(.., is_staging = true)`.
pub(crate) struct StagedCage<S> {
    pub(crate) image: S,
    /// The initial thread registered by `create_thread`.
    pub(crate) main_thread: Option<ThreadId>,
}

/// Staged images keyed by staging ID, until `apply_staging_cage` or
/// `terminate_cage` consumes them.
pub(crate) struct StagingTable<S>(Mutex<Vec<(CageId, StagedCage<S>)>>);

impl<S> StagingTable<S> {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    pub(crate) fn insert(&self, staging_id: CageId, image: S) -> LindResult<()> {
        let mut staged = self.0.lock().unwrap();
        if staged.iter().any(|(id, _)| *id == staging_id) {
            return Err(LindError::Other(Errno::EBUSY));
        }
        staged.push((
            staging_id,
            StagedCage {
                image,
                main_thread: None,
            },
        ));
        Ok(())
    }

    /// Register the initial thread of `staging_id`. Returns `Ok(false)` if
    /// `staging_id` is not staged.
    pub(crate) fn set_main_thread(&self, staging_id: CageId, thread_id: ThreadId) -> LindResult<bool> {
        let mut staged = self.0.lock().unwrap();
        let Some((_, cage)) = staged.iter_mut().find(|(id, _)| *id == staging_id) else {
            return Ok(false);
        };
        if cage.main_thread.is_some() {
            return Err(LindError::Other(Errno::EBUSY));
        }
        cage.main_thread = Some(thread_id);
        Ok(true)
    }

    pub(crate) fn take(&self, staging_id: CageId) -> Option<StagedCage<S>> {
        let mut staged = self.0.lock().unwrap();
        let index = staged.iter().position(|(id, _)| *id == staging_id)?;
        Some(staged.swap_remove(index).1)
    }
}
