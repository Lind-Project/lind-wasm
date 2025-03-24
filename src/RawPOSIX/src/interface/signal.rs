use crate::interface::{cagetable_getref, cagetable_getref_opt, RustAtomicOrdering};
use sysdefs::constants::{SA_NODEFER, SA_RESETHAND, SIG_DFL};

const EPOCH_NORMAL: u64 = 0;
const EPOCH_SIGNAL: u64 = 0xc0ffee;
const EPOCH_KILLED: u64 = 0xdead;

// switch the epoch of the main thread of the cage to "signal" state
// thread safety: this function could possibly be invoked by multiple threads of the same cage
pub fn signal_epoch_trigger(cageid: u64) {
    let cage = cagetable_getref(cageid);

    let threadid_guard = cage.main_threadid.read();
    let main_threadid = *threadid_guard;
    let epoch_handler = cage
        .epoch_handler
        .get(&main_threadid)
        .expect("main threadid does not exist");
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

// switch the epoch of all threads of the cage to "killed" state
// thread safety: this function will only be invoked by main thread of the cage
pub fn epoch_kill_all(cageid: u64) {
    let cage = cagetable_getref(cageid);

    let threadid_guard = cage.main_threadid.read();
    let main_threadid = *threadid_guard;
    // we iterate through the epoch handler of each thread in the cage
    for entry in cage.epoch_handler.iter() {
        if entry.key() == &main_threadid {
            // main thread should be the one invoking this method
            // so main thread could kill itself and we do not need to notify it again
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
}

// get the current epoch state of the thread
// thread safety: this function will only be invoked by main thread of the cage
fn get_epoch_state(cageid: u64, thread_id: u64) -> u64 {
    let cage = cagetable_getref(cageid);
    let epoch_handler = cage
        .epoch_handler
        .get(&(thread_id as i32))
        .expect("threadid does not exist");
    let guard = epoch_handler.read();
    let epoch = *guard;
    // SAFETY: see comment at `signal_epoch_trigger`
    unsafe { *epoch }
}

// check the specified thread with specified cage is in "killed" state
// thread safety: this function could possibly be invoked by multiple threads of the same cage
pub fn thread_check_killed(cageid: u64, thread_id: u64) -> bool {
    let cage = cagetable_getref(cageid);
    // this method should not be invoked if the thread is already killed (i.e. thread is removed from epoch_handler)
    let epoch_handler = cage.epoch_handler.get(&(thread_id as i32)).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    // SAFETY: see comment at `signal_epoch_trigger`
    unsafe { *epoch == EPOCH_KILLED }
}

// reset the epoch of the main thread of the cage to "normal" state
// usually invoked when all the pending signals are handled for the cage
// thread safety: this function will only be invoked by main thread of the cage
pub fn signal_epoch_reset(cageid: u64) {
    let cage = cagetable_getref(cageid);

    let threadid_guard = cage.main_threadid.read();
    let main_threadid = *threadid_guard;
    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    // SAFETY: see comment at `signal_epoch_trigger`
    unsafe {
        *epoch = EPOCH_NORMAL;
    }
}

// manually check if the epoch is not in "normal" state
// useful if we want to do our own epoch check in host
// thread safety: this function will only be invoked by main thread of the cage
pub fn signal_check_trigger(cageid: u64) -> bool {
    let cage = cagetable_getref(cageid);

    let threadid_guard = cage.main_threadid.read();
    let main_threadid = *threadid_guard;

    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    // SAFETY: see comment at `signal_epoch_trigger`
    unsafe { *epoch > EPOCH_NORMAL }
}

// check if the signal of the cage is in blocked state
// thread safety: this function will only be invoked by main thread of the cage
//                but should still work fine if accessed by multiple threads
pub fn signal_check_block(cageid: u64, signo: i32) -> bool {
    let cage = cagetable_getref(cageid);
    let sigset = cage.sigset.load(RustAtomicOrdering::Relaxed);

    // check if the corresponding signal bit is set in sigset
    (sigset & convert_signal_mask(signo)) > 0
}

// retrieve the signal handler for the specified signal of the cage
// if the signal handler does not exist, then return SIG_DFL
// thread safety: this function will only be invoked by main thread of the cage
pub fn signal_get_handler(cageid: u64, signo: i32) -> u32 {
    let cage = cagetable_getref(cageid);
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
pub fn lind_send_signal(cageid: u64, signo: i32) -> bool {
    if let Some(cage) = cagetable_getref_opt(cageid) {
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
    let cage = cagetable_getref(cageid);
    let mut pending_signals = cage.pending_signals.write();
    let sigset = cage.sigset.load(RustAtomicOrdering::Relaxed);

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
                    .fetch_or(sigaction.sa_mask | mask_self, RustAtomicOrdering::Relaxed);

                // restorer is called when the signal handler finishes. It should restore the signal mask
                let restorer = Box::new(move |cageid| {
                    let cage = cagetable_getref(cageid);
                    cage.sigset.store(sigset, RustAtomicOrdering::Relaxed);
                });
                Some((signo, signal_handler, restorer))
            }
            None => {
                // retrieve the signal handler
                // if no signal handler is found, SIG_DFL will be returned
                let signal_handler = signal_get_handler(cageid, signo);
                let restorer = Box::new(move |cageid| {
                    let cage = cagetable_getref(cageid);
                    cage.sigset.store(sigset, RustAtomicOrdering::Relaxed);
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
    let cage = cagetable_getref(cageid);
    let mut pending_signals = cage.pending_signals.write();

    // iterate through each pending signal
    if let Some(index) = pending_signals.iter().position(
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
    let cage = cagetable_getref(cageid);

    // if this is specified as the main thread, then replace the main_threadid field in cage
    if is_mainthread {
        let mut threadid_guard = cage.main_threadid.write();
        *threadid_guard = threadid;
    }
    let epoch_handler = super::RustLock::new(epoch_handler);
    cage.epoch_handler.insert(threadid, epoch_handler);
}

// clean up signal stuff for an exited thread
// return true if this is the last thread in the cage, otherwise return false
pub fn lind_thread_exit(cageid: u64, thread_id: u64) -> bool {
    let cage = cagetable_getref(cageid);
    // lock the main threadid until all the related fields including epoch_handler finishes its updating
    let mut threadid_guard = cage.main_threadid.write();
    let main_threadid = *threadid_guard as u64;

    let mut last_thread = false;

    if thread_id == main_threadid {
        // if main thread exits, we should find a new main thread
        // unless this is the last thread in the cage
        if let Some(entry) = cage
            .epoch_handler
            .iter()
            .find(|entry| *entry.key() as u64 != thread_id)
        {
            let id = *entry.key();
            *threadid_guard = id;

            // we also need to migrate the epoch state to the new thread
            let state = get_epoch_state(cageid, thread_id);
            let new_thread_epoch_handler = entry.value().write();
            let new_thread_epoch = *new_thread_epoch_handler;
            // TODO: we should also make sure the new thread is not in EPOCH_KILLED state.
            // Will be integrated with process exiting fix
            unsafe {
                *new_thread_epoch = state;
            };
        } else {
            // we just exited the last thread in the cage
            last_thread = true;
        }
    }
    // remove the epoch handler of the thread
    cage.epoch_handler
        .remove(&(thread_id as i32))
        .expect("thread id does not exist!");

    last_thread
}

// trigger the epoch if pending signal list is not empty
// This function is invoked only by a newly exec-ed cage
// immediately after it completes its initialization.
// Its purpose is to handle the scenario where Linux resets
// the signal mask but preserves pending signals after exec.
// As a result, the new process may receive signals that were
// pending in the previous process right after it starts.
pub fn signal_may_trigger(cageid: u64) {
    let cage = cagetable_getref(cageid);
    let pending_signals = cage.pending_signals.read();
    if !pending_signals.is_empty() {
        signal_epoch_trigger(cageid);
    }
}
