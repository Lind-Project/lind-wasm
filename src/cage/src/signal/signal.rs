use crate::cage::get_cage;
use parking_lot::RwLock;
use std::sync::atomic::Ordering;
use sysdefs::constants::{SA_NODEFER, SA_RESETHAND, SIG_DFL};
#[cfg(debug_assertions)]
use sysdefs::logging::lind_debug_panic;

const EPOCH_NORMAL: u64 = 0;
const EPOCH_SIGNAL: u64 = 0xc0ffee;
const EPOCH_KILLED: u64 = 0xdead;

// switch the epoch of the main thread of the cage to "signal" state
// thread safety: this function could possibly be invoked by multiple threads of the same cage
pub fn signal_epoch_trigger(cageid: u64) {
    #[cfg(feature = "disable_signals")]
    return;

    #[cfg(not(feature = "disable_signals"))]
    {
        let cage = match get_cage(cageid) {
            Some(c) => c,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!("signal_epoch_trigger: cage {} not found", cageid));
                #[cfg(not(debug_assertions))]
                return;
            }
        };

        let threadid_guard = cage.main_threadid.read();
        let main_threadid = *threadid_guard;
        let epoch_handler = match cage.epoch_handler.get(&main_threadid) {
            Some(h) => h,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!(
                    "signal_epoch_trigger: epoch_handler for thread {} not found",
                    main_threadid
                ));
                #[cfg(not(debug_assertions))]
                return;
            }
        };
        let guard = epoch_handler.write();
        let epoch = *guard;
        // SAFETY: the pointer is locked with write access so no one is able to modify it concurrently
        // However, Potential BUG (TODO): We still need to verify the lifetime of the pointer. This pointer
        // is created by wasmtime and will be destroyed at some point when the wasm instance is destroyed
        // we still need to figure out when is the destroy happening and make sure it is destroyed after the
        // information in rawposix is updated
        unsafe {
            *epoch = EPOCH_SIGNAL;
        }
    }
}

// Atomically claim the exit_group for this cage. Returns true if this
// thread won the race (and should do the full exit_group), false if
// another thread already initiated exit_group (caller should just
// clean up its own thread).
pub fn try_initiate_exit_group(cageid: u64) -> bool {
    use std::sync::atomic::Ordering;
    match get_cage(cageid) {
        Some(cage) => cage
            .exit_group_initiated
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok(),
        None => false, // cage already gone
    }
}

// switch the epoch of all threads of the cage to "killed" state except the caller
// thread safety: this function could be invoked by any thread of the cage (e.g. exit_group)
pub fn epoch_kill_all(cageid: u64, caller_tid: i32) {
    #[cfg(feature = "disable_signals")]
    return;

    #[cfg(not(feature = "disable_signals"))]
    {
        let cage = match get_cage(cageid) {
            Some(c) => c,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!("epoch_kill_all: cage {} not found", cageid));
                #[cfg(not(debug_assertions))]
                return;
            }
        };

        // Set EPOCH_KILLED on every thread except the caller.
        for entry in cage.epoch_handler.iter() {
            if entry.key() == &caller_tid {
                continue;
            }
            let epoch_handler = entry.value();
            let guard = epoch_handler.write();
            let epoch = *guard;
            // SAFETY: see comment at `signal_epoch_trigger`
            unsafe {
                *epoch = EPOCH_KILLED;
            }
        }

        // Send SIGUSR2 to interrupt threads blocked in host syscalls (futex,
        // read, etc.).  The no-op handler causes the syscall to return EINTR,
        // allowing the thread to re-enter WASM where it will see EPOCH_KILLED
        // at the next epoch check and exit via asyncify.
        let my_tid = unsafe { libc::syscall(libc::SYS_gettid) };
        for entry in cage.os_tid_map.iter() {
            let os_tid = *entry.value();
            if os_tid != my_tid {
                unsafe {
                    libc::syscall(libc::SYS_tkill, os_tid as i32, libc::SIGUSR2);
                }
            }
        }
    }
}

// get the current epoch state of the thread
// thread safety: this function will only be invoked by main thread of the cage
fn get_epoch_state(cageid: u64, thread_id: u64) -> u64 {
    #[cfg(feature = "disable_signals")]
    return EPOCH_NORMAL;

    #[cfg(not(feature = "disable_signals"))]
    {
        let cage = match get_cage(cageid) {
            Some(c) => c,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!("get_epoch_state: cage {} not found", cageid));
                #[cfg(not(debug_assertions))]
                return EPOCH_KILLED;
            }
        };
        let epoch_handler = match cage.epoch_handler.get(&(thread_id as i32)) {
            Some(h) => h,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!(
                    "get_epoch_state: epoch_handler for thread {} not found",
                    thread_id
                ));
                #[cfg(not(debug_assertions))]
                return EPOCH_KILLED;
            }
        };
        let guard = epoch_handler.read();
        let epoch = *guard;
        // SAFETY: see comment at `signal_epoch_trigger`
        unsafe { *epoch }
    }
}

// check the specified thread with specified cage is in "killed" state
// thread safety: this function could possibly be invoked by multiple threads of the same cage
pub fn thread_check_killed(cageid: u64, thread_id: u64) -> bool {
    #[cfg(feature = "disable_signals")]
    return false;

    #[cfg(not(feature = "disable_signals"))]
    {
        let cage = match get_cage(cageid) {
            Some(c) => c,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!("thread_check_killed: cage {} not found", cageid));
                #[cfg(not(debug_assertions))]
                return true;
            }
        };
        let epoch_handler = match cage.epoch_handler.get(&(thread_id as i32)) {
            Some(h) => h,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!(
                    "thread_check_killed: epoch_handler for thread {} not found",
                    thread_id
                ));
                #[cfg(not(debug_assertions))]
                return true;
            }
        };
        let guard = epoch_handler.write();
        let epoch = *guard;
        // SAFETY: see comment at `signal_epoch_trigger`
        unsafe { *epoch == EPOCH_KILLED }
    }
}

/// Wait until all threads in the cage except the calling thread have exited.
///
/// Used by exit_group: the exiting thread calls `epoch_kill_all` to mark all
/// other threads for death, then waits here until they've all unwound via
/// asyncify and removed themselves from `epoch_handler`.
///
/// Threads blocked in host syscalls (e.g. futex_wait, read) won't see the
/// epoch kill immediately, so we send them SIGUSR2 to interrupt the blocking
/// call. The no-op handler causes the syscall to return EINTR, allowing the
/// thread to re-enter wasm where it will see the epoch and exit.
pub fn wait_all_threads_exited(cageid: u64, _except_tid: u64) {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!(
                "wait_all_threads_exited: cage {} not found",
                cageid
            ));
            #[cfg(not(debug_assertions))]
            return;
        }
    };
    let my_tid = unsafe { libc::syscall(libc::SYS_gettid) };

    loop {
        if cage.epoch_handler.len() <= 1 {
            return;
        }

        // Send SIGUSR2 to all other threads to interrupt any blocking host
        // syscalls. We do this in the loop because a thread might re-enter
        // a blocking call before it sees the epoch kill.
        for entry in cage.os_tid_map.iter() {
            let os_tid = *entry.value();
            if os_tid != my_tid {
                unsafe {
                    libc::syscall(libc::SYS_tkill, os_tid as i32, libc::SIGUSR2);
                }
            }
        }

        std::thread::yield_now();
    }
}

// reset the epoch of the main thread of the cage to "normal" state
// usually invoked when all the pending signals are handled for the cage
// thread safety: this function will only be invoked by main thread of the cage
pub fn signal_epoch_reset(cageid: u64) {
    #[cfg(feature = "disable_signals")]
    return;

    #[cfg(not(feature = "disable_signals"))]
    {
        let cage = match get_cage(cageid) {
            Some(c) => c,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!("signal_epoch_reset: cage {} not found", cageid));
                #[cfg(not(debug_assertions))]
                return;
            }
        };

        let threadid_guard = cage.main_threadid.read();
        let main_threadid = *threadid_guard;
        let epoch_handler = match cage.epoch_handler.get(&main_threadid) {
            Some(h) => h,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!(
                    "signal_epoch_reset: epoch_handler for thread {} not found",
                    main_threadid
                ));
                #[cfg(not(debug_assertions))]
                return;
            }
        };
        let guard = epoch_handler.write();
        let epoch = *guard;
        // SAFETY: see comment at `signal_epoch_trigger`
        unsafe {
            *epoch = EPOCH_NORMAL;
        }
    }
}

// manually check if the epoch is not in "normal" state
// useful if we want to do our own epoch check in host
// thread safety: this function will only be invoked by main thread of the cage
pub fn signal_check_trigger(cageid: u64) -> bool {
    #[cfg(feature = "disable_signals")]
    return false;

    #[cfg(not(feature = "disable_signals"))]
    {
        let cage = match get_cage(cageid) {
            Some(c) => c,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!("signal_check_trigger: cage {} not found", cageid));
                #[cfg(not(debug_assertions))]
                return false;
            }
        };

        let threadid_guard = cage.main_threadid.read();
        let main_threadid = *threadid_guard;

        let epoch_handler = match cage.epoch_handler.get(&main_threadid) {
            Some(h) => h,
            None => {
                #[cfg(debug_assertions)]
                lind_debug_panic(&format!(
                    "signal_check_trigger: epoch_handler for thread {} not found",
                    main_threadid
                ));
                #[cfg(not(debug_assertions))]
                return false;
            }
        };
        let guard = epoch_handler.write();
        let epoch = *guard;
        // SAFETY: see comment at `signal_epoch_trigger`
        unsafe { *epoch > EPOCH_NORMAL }
    }
}

// check if the signal of the cage is in blocked state
// thread safety: this function will only be invoked by main thread of the cage
//                but should still work fine if accessed by multiple threads
pub fn signal_check_block(cageid: u64, signo: i32) -> bool {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!("signal_check_block: cage {} not found", cageid));
            #[cfg(not(debug_assertions))]
            return false;
        }
    };
    let sigset = cage.sigset.load(Ordering::Relaxed);

    // check if the corresponding signal bit is set in sigset
    (sigset & convert_signal_mask(signo)) > 0
}

// retrieve the signal handler for the specified signal of the cage
// if the signal handler does not exist, then return SIG_DFL
// thread safety: this function will only be invoked by main thread of the cage
pub fn signal_get_handler(cageid: u64, signo: i32) -> u32 {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!("signal_get_handler: cage {} not found", cageid));
            #[cfg(not(debug_assertions))]
            return SIG_DFL as u32;
        }
    };
    let handler = match cage.signalhandler.get(&signo) {
        Some(action_struct) => {
            action_struct.sa_handler // if we have a handler and its not blocked return it
        }
        None => SIG_DFL as u32, // if we dont have a handler return SIG_DFL
    };
    handler
}

// send specified signal to the cage, return value indicates whether the cage exists
// thread safety: this function could possibly be invoked by multiple threads of the same cage
// NOTE: signo MUST be checked to make sure it's valid before passing to this function,
//       otherwise would cause undefined behavior in release build
pub fn lind_send_signal(cageid: u64, signo: i32) -> bool {
    debug_assert!(
        signo > 0 && signo < 32,
        "invalid signal number passed to lind_send_signal"
    );

    if let Some(cage) = get_cage(cageid) {
        // From https://man7.org/linux/man-pages/man2/kill.2.html
        // If sig is 0, then no signal is sent, but existence and permission
        // checks are still performed
        if signo > 0 {
            // if the sent signal has the default disposition and its default behavior is SIG_DFL
            // let's just ignore the signal
            if signal_get_handler(cageid, signo) == SIG_DFL.try_into().unwrap()
                && sysdefs::constants::signal_default_handler_dispatcher(signo)
                    == sysdefs::constants::SignalDefaultHandler::Ignore
            {
                return true;
            }

            let mut pending_signals = cage.pending_signals.write();
            // TODO: currently we are queuing the same signals instead of merging the same signal
            // this is different from linux which always merge the same signal if they havn't been handled yet
            // we queue the signals for now because our epoch based signal implementation could have much longer
            // gap for signal checkings than linux. We need to finally decide whether do the queuing or merging
            // in the future, probably based on some experimental data
            pending_signals.push(signo);

            // we only trigger epoch if the signal is not blocked
            if !signal_check_block(cageid, signo) {
                signal_epoch_trigger(cageid);
            }
        }

        true
    } else {
        false
    }
}

pub fn convert_signal_mask(signo: i32) -> u64 {
    (1 << (signo - 1)) as u64
}

// retrieve the first unblocked signal in the pending signal list
// returns an optional tuple where the first element is the signal number
// the second element is the signal handler
// and the third element is the signal mask restore callback function
// thread safety: this function will only be invoked by main thread of the cage
pub fn lind_get_first_signal(cageid: u64) -> Option<(i32, u32, Box<dyn Fn(u64)>)> {
    let cage = get_cage(cageid)?;
    let mut pending_signals = cage.pending_signals.write();
    let sigset = cage.sigset.load(Ordering::Relaxed);

    // we iterate through signal and retrieve the first unblocked signals in the pending list
    if let Some(index) = pending_signals.iter().position(
        |&signo| (sigset & convert_signal_mask(signo)) == 0, // check if signal is blocked
    ) {
        // retrieve the signal number
        let signo = pending_signals.remove(index);
        // retrieve the corresponding signal handler
        match cage.signalhandler.get_mut(&signo) {
            Some(mut sigaction) => {
                // if sigprocmask is called during the execution of the signal handler
                // the signal mask will not be perseved once handler is finished

                // by default, we block the same signal during its execution
                let mut mask_self = convert_signal_mask(signo);
                let signal_handler = sigaction.sa_handler;
                // if SA_RESETHAND is set, we reset the signal handler to default for this signal
                if sigaction.sa_flags as u32 & SA_RESETHAND > 0 {
                    sigaction.sa_handler = SIG_DFL as u32;
                }

                // if SA_NODEFER is set, we allow the same signal to interrupt itself
                if sigaction.sa_flags as u32 & SA_NODEFER > 0 {
                    mask_self = 0;
                }
                // temporily update the signal mask
                cage.sigset
                    .fetch_or(sigaction.sa_mask | mask_self, Ordering::Relaxed);

                // restorer is called when the signal handler finishes. It should restore the signal mask
                let restorer = Box::new(move |cageid| {
                    if let Some(cage) = get_cage(cageid) {
                        cage.sigset.store(sigset, Ordering::Relaxed);
                    }
                });
                Some((signo, signal_handler, restorer))
            }
            None => {
                // retrieve the signal handler
                // if no signal handler is found, SIG_DFL will be returned
                let signal_handler = signal_get_handler(cageid, signo);
                let restorer = Box::new(move |cageid| {
                    if let Some(cage) = get_cage(cageid) {
                        cage.sigset.store(sigset, Ordering::Relaxed);
                    }
                });
                Some((signo, signal_handler, restorer))
            }
        }
    } else {
        // if there is no pending unblocked signal, we return None
        None
    }
}

// check if there is any pending unblocked signals
// return true if no pending unblocked signals are found
// thread safety: this function will only be invoked by main thread of the cage
pub fn lind_check_no_pending_signal(cageid: u64) -> bool {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!(
                "lind_check_no_pending_signal: cage {} not found",
                cageid
            ));
            #[cfg(not(debug_assertions))]
            return true;
        }
    };
    let pending_signals = cage.pending_signals.read();

    // iterate through each pending signal
    if let Some(_index) = pending_signals.iter().position(
        // check if the signal is blocked
        |&signo| !signal_check_block(cageid, signo),
    ) {
        false
    } else {
        true
    }
}

// initialize the signal for a new thread
// thread safety: this function could possibly be invoked by multiple threads of the same cage
pub fn lind_signal_init(cageid: u64, epoch_handler: *mut u64, threadid: i32, is_mainthread: bool) {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!("lind_signal_init: cage {} not found", cageid));
            #[cfg(not(debug_assertions))]
            return;
        }
    };

    // if this is specified as the main thread, then replace the main_threadid field in cage
    if is_mainthread {
        let mut threadid_guard = cage.main_threadid.write();
        *threadid_guard = threadid;
    }
    let epoch_handler = RwLock::new(epoch_handler);
    cage.epoch_handler.insert(threadid, epoch_handler);

    // Store the OS thread ID so epoch_kill_all can send SIGUSR2 to interrupt
    // threads blocked in host syscalls
    let os_tid = unsafe { libc::syscall(libc::SYS_gettid) };
    cage.os_tid_map.insert(threadid, os_tid);
}

// clean up signal stuff for an exited thread
// return true if this is the last thread in the cage, otherwise return false
pub fn lind_thread_exit(cageid: u64, thread_id: u64) -> bool {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!("lind_thread_exit: cage {} not found", cageid));
            #[cfg(not(debug_assertions))]
            return false;
        }
    };
    // lock the main threadid until all the related fields including epoch_handler finishes its updating
    let mut threadid_guard = cage.main_threadid.write();
    let main_threadid = *threadid_guard as u64;

    // Save epoch state BEFORE removal (needed for potential migration).
    // Skipped entirely when signals are disabled since the epoch pointer is null.
    #[cfg(not(feature = "disable_signals"))]
    let saved_epoch_state = if thread_id == main_threadid {
        get_epoch_state(cageid, thread_id)
    } else {
        EPOCH_NORMAL
    };

    // Remove self FIRST so the subsequent is_empty() check is definitive:
    // after this point, our entry is gone and any remaining entries belong
    // to threads that have not yet exited.
    cage.epoch_handler.remove(&(thread_id as i32));
    cage.os_tid_map.remove(&(thread_id as i32));

    // If no more threads remain, this is the last thread.
    if cage.epoch_handler.is_empty() {
        return true;
    }

    // Not the last thread.  If this was the main thread, migrate main role.
    if thread_id == main_threadid {
        if let Some(entry) = cage.epoch_handler.iter().next() {
            let id = *entry.key();
            *threadid_guard = id;

            // Migrate epoch state to the new main thread.
            #[cfg(not(feature = "disable_signals"))]
            {
                // if the exiting thread has pending signals, migrate to the newly assigned main-thread
                // NOTE: below implementation of signal migration between threads is based on the assumption
                // that EPOCH_KILLED state will only occur when all the threads is in EPOCH_KILLED state
                // which holds true for now since EPOCH_KILLED is currently only used in epoch_kill_all
                if saved_epoch_state == EPOCH_SIGNAL {
                    let new_thread_epoch_handler = entry.value().write();
                    let new_thread_epoch = *new_thread_epoch_handler;
                    unsafe {
                        // make sure not to overwrite EPOCH_KILLED
                        if *new_thread_epoch != EPOCH_KILLED {
                            *new_thread_epoch = saved_epoch_state;
                        }
                    };
                }
            }
        }
    }

    false
}

// trigger the epoch if pending signal list is not empty
// This function is invoked only by a newly exec-ed cage
// immediately after it completes its initialization.
// Its purpose is to handle the scenario where Linux resets
// the signal mask but preserves pending signals after exec.
// As a result, the new process may receive signals that were
// pending in the previous process right after it starts.
pub fn signal_may_trigger(cageid: u64) {
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(debug_assertions)]
            lind_debug_panic(&format!("signal_may_trigger: cage {} not found", cageid));
            #[cfg(not(debug_assertions))]
            return;
        }
    };
    let pending_signals = cage.pending_signals.read();
    if !pending_signals.is_empty() {
        signal_epoch_trigger(cageid);
    }
}
