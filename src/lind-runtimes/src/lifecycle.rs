//! Side channels between Core's lifecycle shims (lind-boot) and the runtimes
//! for values the draft Lind-IF lifecycle operations do not carry yet: the
//! clone syscall result and a thread's exit status. Also holds the staged
//! exec images shared by the runtimes' `init_cage`/`apply_staging_cage`.

use lind_if::{CageId, Errno, LindError, LindResult, ThreadId};
use std::cell::Cell;
use std::sync::Mutex;

thread_local! {
    static CLONE_RESULT: Cell<Option<i32>> = const { Cell::new(None) };
    static EXIT_STATUS: Cell<i32> = const { Cell::new(0) };
}

/// Clear the clone result before Core calls `duplicate_thread`.
pub fn begin_clone() {
    CLONE_RESULT.with(|result| result.set(None));
}

/// Take the clone syscall result recorded for this continuation.
///
/// `LindRuntime::duplicate_thread` returns no value, so Core supplies the
/// default parent result (the child cage ID for fork). Until Core allocates
/// thread IDs, a runtime records the TID it assigned to a new thread. A runtime
/// whose child continuation returns through the calling handler frames (MPK
/// resumes children on a copied supervisor stack) records the child result 0
/// on the child's host thread.
pub fn take_clone_result() -> Option<i32> {
    CLONE_RESULT.with(Cell::take)
}

pub(crate) fn set_clone_result(value: i32) {
    CLONE_RESULT.with(|result| result.set(Some(value)));
}

/// Convert a negative errno return value of a runtime's internal path.
pub(crate) fn errno_error(ret: i32) -> LindError {
    LindError::Other(Errno::from_discriminant(-ret).unwrap_or(Errno::EAGAIN))
}

/// Record the exit status of the calling thread before Core calls
/// `LindRuntime::terminate_thread` on it.
///
/// `terminate_thread` carries no status; it becomes the thread's (and, for
/// the cage's last thread, the host's) exit value. The process-level status
/// is recorded in Core by exit_group.
pub fn begin_thread_exit(status: i32) {
    EXIT_STATUS.with(|exit_status| exit_status.set(status));
}

pub(crate) fn take_thread_exit_status() -> i32 {
    EXIT_STATUS.with(|exit_status| exit_status.replace(0))
}

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
