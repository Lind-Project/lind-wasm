use crate::interface::{cagetable_getref, cagetable_getref_opt, RustAtomicOrdering};

pub fn signal_epoch_trigger(cageid: u64) {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;
    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch = 1;
    }
}

pub fn signal_epoch_trigger_all(cageid: u64) {
    println!("-----signal_epoch_trigger_all");
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;
    for entry in cage.epoch_handler.iter() {
        if entry.key() == &main_threadid {
            // main thread should be the one invoke this method
            // which means its epoch should already in "trigger" state
            continue;
        }
        println!("-----signal_epoch_trigger_all, trigger epoch for {}", entry.key());
        let epoch_handler = entry.value();
        let guard = epoch_handler.write();
        let epoch = *guard;
        unsafe {
            *epoch = 2;
        }
    }
}

pub fn thread_check_killed(cageid: u64, thread_id: u64) -> bool {
    let cage = cagetable_getref(cageid);
    let epoch_handler = cage.epoch_handler.get(&(thread_id as i32)).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch == 2
    }
}

pub fn signal_epoch_reset(cageid: u64) {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;
    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch = 0;
    }
}

pub fn signal_check_trigger(cageid: u64) -> bool {
    let cage = cagetable_getref(cageid);
    let main_threadid = cage.main_threadid.load(RustAtomicOrdering::Relaxed) as i32;

    let epoch_handler = cage.epoch_handler.get(&main_threadid).unwrap();
    let guard = epoch_handler.write();
    let epoch = *guard;
    unsafe {
        *epoch > 0
    }
}

pub fn signal_check_block(cageid: u64, signo: i32) -> bool {
    let cage = cagetable_getref(cageid);
    let sigset = cage.sigset.load(RustAtomicOrdering::Relaxed);
    
    (sigset & ((1 << (signo - 1)) as u64)) > 0
}

pub fn signal_get_handler(cageid: u64, signo: i32) -> u32 {
    let cage = cagetable_getref(cageid);
    let handler = match cage.signalhandler.get(&signo) {
        Some(action_struct) => {
            action_struct.sa_handler // if we have a handler and its not blocked return it
        }
        None => 0, // if we dont have a handler return 0
    };
    handler
}

pub fn signal_reset_handler(cageid: u64, signo: i32) {
    let cage = cagetable_getref(cageid);
    let mut act = match cage.signalhandler.get(&signo) {
        Some(action_struct) => {
            action_struct.clone()
        }
        None => { return; },
    };
    act.sa_handler = 0;
    cage.signalhandler.insert(signo, act);
}

pub fn lind_send_signal(cageid: u64, signo: i32) -> bool {
    if let Some(cage) = cagetable_getref_opt(cageid) {
        let mut pending_signals = cage.pending_signals.write();
        pending_signals.push(signo);

        if !signal_check_block(cageid, signo) {
            signal_epoch_trigger(cageid);
        }
        
        true
    } else {
        false
    }
}
