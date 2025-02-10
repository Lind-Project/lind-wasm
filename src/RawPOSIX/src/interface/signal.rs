use crate::{constants::SIG_DFL, interface::{cagetable_getref, cagetable_getref_opt, RustAtomicOrdering}};

const EPOCH_NORMAL: u64 = 0;
const EPOCH_SIGNAL: u64 = 1;
const EPOCH_KILLED: u64 = 2;

// switch the epoch of the main thread of the cage to "signal" state
pub fn signal_epoch_trigger(cageid: u64) {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;
    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch = EPOCH_SIGNAL;
    }
}

// swtich the epoch of all threads of the cage to "killed" state
pub fn epoch_kill_all(cageid: u64) {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;
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
        unsafe {
            *epoch = EPOCH_KILLED;
        }
    }
}

// check the specified thread with specified cage is in "killed" state
pub fn thread_check_killed(cageid: u64, thread_id: u64) -> bool {
    let cage = cagetable_getref(cageid);
    let epoch_handler = cage.epoch_handler.get(&(thread_id as i32)).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch == EPOCH_KILLED
    }
}

// reset the epoch of the main thread of the cage to "normal" state
// usually invoked when all the pending signals are handled for the cage
pub fn signal_epoch_reset(cageid: u64) {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;
    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch = EPOCH_NORMAL;
    }
}

// manually check if the epoch is not in "normal" state
// useful if we want to do our own epoch check in host
pub fn signal_check_trigger(cageid: u64) -> bool {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;

    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch > EPOCH_NORMAL
    }
}

// check if the signal of the cage is in blocked state
pub fn signal_check_block(cageid: u64, signo: i32) -> bool {
    let cage = cagetable_getref(cageid);
    let sigset = cage.sigset.load(RustAtomicOrdering::Relaxed);
    
    // check if the corresponding signal bit is set in sigset 
    (sigset & ((1 << (signo - 1)) as u64)) > 0
}

// retrieve the signal handler for the specified signal of the cage
// if the signal handler does not exist, then return
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

// reset the signal handler of the specified signal of the cage to SIG_DFL
// used by SA_RESETHAND flag
pub fn signal_reset_handler(cageid: u64, signo: i32) {
    let cage = cagetable_getref(cageid);
    match cage.signalhandler.get_mut(&signo) {
        Some(mut action_struct) => {
            action_struct.sa_handler = SIG_DFL as u32;
        }
        None => { return; },
    };
}

// send specified signal to the cage, return value indicates whether the cage exists
pub fn lind_send_signal(cageid: u64, signo: i32) -> bool {
    if let Some(cage) = cagetable_getref_opt(cageid) {
        let mut pending_signals = cage.pending_signals.write();
        // queue the signal
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
