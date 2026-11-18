//! Shared signal notification for runtimes that observe signals through the
//! per-thread epoch and the SIGUSR2 interrupt.

use lind_if::{CageId, LindResult, ThreadId};

/// If Core has a deliverable (pending, unblocked) signal, marks `thread_id`'s
/// epoch so its next check obtains it from Core, and interrupts the thread in
/// case it is blocked in a host syscall.
///
/// Notifications for still-blocked signals are ignored: a raised epoch makes
/// blocking RawPOSIX calls return EINTR, and SIGUSR2 interrupts host syscalls.
/// Core notifies again once such a signal is unblocked.
pub(crate) fn notify_epoch_thread(cage_id: CageId, thread_id: ThreadId) -> LindResult<()> {
    if cage::lind_check_no_pending_signal(cage_id) {
        return Ok(());
    }
    cage::signal_epoch_trigger_thread(cage_id, thread_id as u64);
    cage::signal_interrupt_thread(cage_id, thread_id as u64);
    Ok(())
}
