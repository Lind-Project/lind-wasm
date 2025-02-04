#![allow(dead_code)]

// System related system calls
use crate::constants::{
    DEFAULT_GID, DEFAULT_UID, ITIMER_REAL, NOFILE_CUR, NOFILE_MAX, RLIMIT_NOFILE, RLIMIT_STACK, SEM_VALUE_MAX, SHMMAX, SHMMIN, SHM_DEST, SHM_RDONLY, SIGNAL_MAX, SIG_BLOCK, SIG_MAX, SIG_SETMASK, SIG_UNBLOCK, STACK_CUR, STACK_MAX
};

use crate::interface;
use crate::safeposix::cage;
use crate::safeposix::cage::*;
use crate::safeposix::shm::*;

use crate::fdtables;

use libc::*;

use std::io::Write;
use std::io;

use std::sync::Arc as RustRfc;

impl Cage {
    fn unmap_shm_mappings(&self) {
        //unmap shm mappings on exit or exec
        for rev_mapping in self.rev_shm.lock().iter() {
            let shmid = rev_mapping.1;
            let metadata = &SHM_METADATA;
            match metadata.shmtable.entry(shmid) {
                interface::RustHashEntry::Occupied(mut occupied) => {
                    let segment = occupied.get_mut();
                    segment.shminfo.shm_nattch -= 1;
                    segment.shminfo.shm_dtime = interface::timestamp() as isize;
                    segment.attached_cages.remove(&self.cageid);

                    if segment.rmid && segment.shminfo.shm_nattch == 0 {
                        let key = segment.key;
                        occupied.remove_entry();
                        metadata.shmkeyidtable.remove(&key);
                    }
                }
                interface::RustHashEntry::Vacant(_) => {
                    panic!("Shm entry not created for some reason");
                }
            };
        }
    }

    pub fn fork_syscall(&self, child_cageid: u64) -> i32 {
        // Modify the fdtable manually 
        fdtables::copy_fdtable_for_cage(self.cageid, child_cageid).unwrap();
        
        //construct a new mutex in the child cage where each initialized mutex is in the parent cage
        let mutextable = self.mutex_table.read();
        let mut new_mutex_table = vec![];
        for elem in mutextable.iter() {
            if elem.is_some() {
                let new_mutex_result = interface::RawMutex::create();
                match new_mutex_result {
                    Ok(new_mutex) => new_mutex_table.push(Some(interface::RustRfc::new(new_mutex))),
                    Err(_) => {
                        match Errno::from_discriminant(interface::get_errno()) {
                            Ok(i) => {
                                return syscall_error(
                                    i,
                                    "fork",
                                    "The libc call to pthread_mutex_init failed!",
                                );
                            }
                            Err(()) => {
                                panic!("Unknown errno value from pthread_mutex_init returned!")
                            }
                        };
                    }
                }
            } else {
                new_mutex_table.push(None);
            }
        }
        drop(mutextable);

        //construct a new condvar in the child cage where each initialized condvar is in the parent cage
        let cvtable = self.cv_table.read();
        let mut new_cv_table = vec![];
        for elem in cvtable.iter() {
            if elem.is_some() {
                let new_cv_result = interface::RawCondvar::create();
                match new_cv_result {
                    Ok(new_cv) => new_cv_table.push(Some(interface::RustRfc::new(new_cv))),
                    Err(_) => {
                        match Errno::from_discriminant(interface::get_errno()) {
                            Ok(i) => {
                                return syscall_error(
                                    i,
                                    "fork",
                                    "The libc call to pthread_cond_init failed!",
                                );
                            }
                            Err(()) => {
                                panic!("Unknown errno value from pthread_cond_init returned!")
                            }
                        };
                    }
                }
            } else {
                new_cv_table.push(None);
            }
        }
        drop(cvtable);

        // we grab the parent cages main threads sigset and store it at 0
        // we do this because we haven't established a thread for the cage yet, and dont have a threadid to store it at
        // this way the child can initialize the sigset properly when it establishes its own mainthreadid
        // let newsigset = interface::RustHashMap::new();
        if !interface::RUSTPOSIX_TESTSUITE.load(interface::RustAtomicOrdering::Relaxed) {
            // we don't add these for the test suite
            // BUG: Signals are commented out until we add them to lind-wasm
            // let mainsigsetatomic = self
            //     .sigset
            //     .get(
            //         &self
            //             .main_threadid
            //             .load(interface::RustAtomicOrdering::Relaxed),
            //     )
            //     .unwrap();
            // let mainsigset = interface::RustAtomicU64::new(
            //     mainsigsetatomic.load(interface::RustAtomicOrdering::Relaxed),
            // );
            // newsigset.insert(0, mainsigset);
        }

        /*
         *  Construct a new semaphore table in child cage which equals to the one in the parent cage
         */
        let semtable = &self.sem_table;
        let new_semtable: interface::RustHashMap<
            u32,
            interface::RustRfc<interface::RustSemaphore>,
        > = interface::RustHashMap::new();
        // Loop all pairs
        for pair in semtable.iter() {
            new_semtable.insert((*pair.key()).clone(), pair.value().clone());
        }
        let parent_vmmap = self.vmmap.read();
        let new_vmmap = parent_vmmap.clone();

        let cageobj = Cage {
            cageid: child_cageid,
            cwd: interface::RustLock::new(self.cwd.read().clone()),
            parent: self.cageid,
            cancelstatus: interface::RustAtomicBool::new(false),
            // This happens because self.getgid tries to copy atomic value which does not implement "Copy" trait; self.getgid.load returns i32.
            getgid: interface::RustAtomicI32::new(
                self.getgid.load(interface::RustAtomicOrdering::Relaxed),
            ),
            getuid: interface::RustAtomicI32::new(
                self.getuid.load(interface::RustAtomicOrdering::Relaxed),
            ),
            getegid: interface::RustAtomicI32::new(
                self.getegid.load(interface::RustAtomicOrdering::Relaxed),
            ),
            geteuid: interface::RustAtomicI32::new(
                self.geteuid.load(interface::RustAtomicOrdering::Relaxed),
            ),
            rev_shm: interface::Mutex::new((*self.rev_shm.lock()).clone()),
            mutex_table: interface::RustLock::new(new_mutex_table),
            cv_table: interface::RustLock::new(new_cv_table),
            sem_table: new_semtable,
            thread_table: interface::RustHashMap::new(),
            signalhandler: self.signalhandler.clone(),
            sigset: interface::RustAtomicU64::new(
                self.sigset.load(interface::RustAtomicOrdering::Relaxed),
            ),
            pending_signals: interface::RustLock::new(vec![]),
            signal_triggerable: interface::RustAtomicBool::new(true),
            epoch_handler: interface::RustLock::new(0 as *mut u64),
            main_threadid: interface::RustAtomicU64::new(0),
            interval_timer: interface::IntervalTimer::new(child_cageid),
            vmmap: interface::RustLock::new(new_vmmap), // Initialize empty virtual memory map for new process
            zombies: interface::RustLock::new(vec![]),
            child_num: interface::RustAtomicU64::new(0),
        };

        // increment child counter for parent
        self.child_num.fetch_add(1, interface::RustAtomicOrdering::SeqCst);


        let shmtable = &SHM_METADATA.shmtable;
        //update fields for shared mappings in cage
        for rev_mapping in cageobj.rev_shm.lock().iter() {
            let mut shment = shmtable.get_mut(&rev_mapping.1).unwrap();
            shment.shminfo.shm_nattch += 1;
            let refs = shment.attached_cages.get(&self.cageid).unwrap();
            let childrefs = refs.clone();
            drop(refs);
            shment.attached_cages.insert(child_cageid, childrefs);
        }

        interface::cagetable_insert(child_cageid, cageobj);

        0
    }

    /*
    *   exec() will only return if error happens 
    */
    pub fn exec_syscall(&self, child_cageid: u64) -> i32 {
        // Empty fd with flag should_cloexec 
        fdtables::empty_fds_for_exec(self.cageid);
        // Add the new one to fdtable
        let _ = fdtables::copy_fdtable_for_cage(self.cageid, child_cageid);
        // Delete the original one
        let _newfdtable = fdtables::remove_cage_from_fdtable(self.cageid);

        interface::cagetable_remove(self.cageid);

        self.unmap_shm_mappings();

        let zombies = self.zombies.read();
        let cloned_zombies = zombies.clone();
        let child_num = self.child_num.load(interface::RustAtomicOrdering::Relaxed);
        drop(zombies);

        // we grab the parent cages main threads sigset and store it at 0
        // this way the child can initialize the sigset properly when it establishes its own mainthreadid
        // let newsigset = interface::RustHashMap::new();
        if !interface::RUSTPOSIX_TESTSUITE.load(interface::RustAtomicOrdering::Relaxed) {
            // we don't add these for the test suite
            // BUG: Signals are commented out until we add them to lind-wasm
            // let mainsigsetatomic = self
            //     .sigset
            //     .get(
            //         &self
            //             .main_threadid
            //             .load(interface::RustAtomicOrdering::Relaxed),
            //     )
            //     .unwrap();
            // let mainsigset = interface::RustAtomicU64::new(
            //     mainsigsetatomic.load(interface::RustAtomicOrdering::Relaxed),
            // );
            // newsigset.insert(0, mainsigset);
        }

        let newcage = Cage {
            cageid: child_cageid,
            cwd: interface::RustLock::new(self.cwd.read().clone()),
            parent: self.parent,
            cancelstatus: interface::RustAtomicBool::new(false),
            getgid: interface::RustAtomicI32::new(-1),
            getuid: interface::RustAtomicI32::new(-1),
            getegid: interface::RustAtomicI32::new(-1),
            geteuid: interface::RustAtomicI32::new(-1),
            rev_shm: interface::Mutex::new(vec![]),
            mutex_table: interface::RustLock::new(vec![]),
            cv_table: interface::RustLock::new(vec![]),
            sem_table: interface::RustHashMap::new(),
            thread_table: interface::RustHashMap::new(),
            signalhandler: interface::RustHashMap::new(),
            sigset: interface::RustAtomicU64::new(
                self.sigset.load(interface::RustAtomicOrdering::Relaxed),
            ),
            pending_signals: interface::RustLock::new(
                self.pending_signals.read().clone(),
            ),
            signal_triggerable: interface::RustAtomicBool::new(true),
            epoch_handler: interface::RustLock::new(0 as *mut u64),
            main_threadid: interface::RustAtomicU64::new(0),
            interval_timer: self.interval_timer.clone_with_new_cageid(child_cageid),
            vmmap: interface::RustLock::new(Vmmap::new()),  // Fresh clean vmmap
            // when a process exec-ed, its child relationship should be perserved
            zombies: interface::RustLock::new(cloned_zombies),
            child_num: interface::RustAtomicU64::new(child_num),
        };
        //wasteful clone of fdtable, but mutability constraints exist

        interface::cagetable_insert(child_cageid, newcage);
        0
    }

    pub fn exit_syscall(&self, status: i32) -> i32 {
        //flush anything left in stdout
        interface::flush_stdout();
        self.unmap_shm_mappings();

        let _ = fdtables::remove_cage_from_fdtable(self.cageid);

        //may not be removable in case of lindrustfinalize, we don't unwrap the remove result
        interface::cagetable_remove(self.cageid);

        // if the cage has parent
        if self.parent != self.cageid {
            let parent_cage = interface::cagetable_getref_opt(self.parent);
            // if parent hasn't exited yet
            if let Some(parent) = parent_cage {
                // decrement parent's child counter
                parent.child_num.fetch_sub(1, interface::RustAtomicOrdering::SeqCst);

                // push the exit status to parent's zombie list
                let mut zombie_vec = parent.zombies.write();
                zombie_vec.push(Zombie { cageid: self.cageid, exit_code: status });
            } else {
                // if parent already exited
                // BUG: we currently do not handle the situation where a parent has exited already
            }
        }

        // Trigger SIGCHLD
        if !interface::RUSTPOSIX_TESTSUITE.load(interface::RustAtomicOrdering::Relaxed) {
            // dont trigger SIGCHLD for test suite
            // BUG: Signals are commented out until we add them to lind-wasm
            // if self.cageid != self.parent {
            //     interface::lind_kill_from_id(self.parent, libc::SIGCHLD);
            // }
        }

        //fdtable will be dropped at end of dispatcher scope because of Arc
        status
    }


    //------------------------------------WAITPID SYSCALL------------------------------------
    /*
    *   waitpid() will return the cageid of waited cage, or 0 when WNOHANG is set and there is no cage already exited
    *   waitpid_syscall utilizes the zombie list stored in cage struct. When a cage exited, a zombie entry will be inserted
    *   into the end of its parent's zombie list. Then when parent wants to wait for any of child, it could just check its
    *   zombie list and retrieve the first entry from it (first in, first out).
    */
    pub fn waitpid_syscall(&self, cageid: i32, status: &mut i32, options: i32) -> i32 {
        let mut zombies = self.zombies.write();
        let child_num = self.child_num.load(interface::RustAtomicOrdering::Relaxed);

        // if there is no pending zombies to wait, and there is no active child, return ECHILD
        if zombies.len() == 0 && child_num == 0 {
            return syscall_error(Errno::ECHILD, "waitpid", "no existing unwaited-for child processes");
        }

        let mut zombie_opt: Option<Zombie> = None;

        // cageid <= 0 means wait for ANY child
        // cageid < 0 actually refers to wait for any child process whose process group ID equals -pid
        // but we do not have the concept of process group in lind, so let's just treat it as cageid == 0
        if cageid <= 0 {
            loop {
                if zombies.len() == 0 && (options & libc::WNOHANG > 0) {
                    // if there is no pending zombies and WNOHANG is set
                    // return immediately
                    return 0;
                } else if zombies.len() == 0 {
                    // if there is no pending zombies and WNOHANG is not set
                    // then we need to wait for children to exit
                    // drop the zombies list before sleep to avoid deadlock
                    drop(zombies);
                    // TODO: replace busy waiting with more efficient mechanism
                    interface::lind_yield();
                    // after sleep, get the write access of zombies list back
                    zombies = self.zombies.write();
                    continue;
                } else {
                    // there are zombies avaliable
                    // let's retrieve the first zombie
                    zombie_opt = Some(zombies.remove(0));
                    break;
                }
            }
        }
        // if cageid is specified, then we need to look up the zombie list for the id
        else {
            // first let's check if the cageid is in the zombie list
            if let Some(index) = zombies.iter().position(|zombie| zombie.cageid == cageid as u64) {
                // find the cage in zombie list, remove it from the list and break
                zombie_opt = Some(zombies.remove(index));
            } else {
                // if the cageid is not in the zombie list, then we know either
                // 1. the child is still running, or
                // 2. the cage has exited, but it is not the child of this cage, or
                // 3. the cage does not exist
                // we need to make sure the child is still running, and it is the child of this cage
                let child = interface::cagetable_getref_opt(cageid as u64);
                if let Some(child_cage) = child {
                    // make sure the child's parent is correct
                    if child_cage.parent != self.cageid {
                        return syscall_error(Errno::ECHILD, "waitpid", "waited cage is not the child of the cage");
                    }
                } else {
                    // cage does not exist
                    return syscall_error(Errno::ECHILD, "waitpid", "cage does not exist");
                }

                // now we have verified that the cage exists and is the child of the cage
                loop {
                    // the cage is not in the zombie list
                    // we need to wait for the cage to actually exit

                    // drop the zombies list before sleep to avoid deadlock
                    drop(zombies);
                    // TODO: replace busy waiting with more efficient mechanism
                    interface::lind_yield();
                    // after sleep, get the write access of zombies list back
                    zombies = self.zombies.write();

                    // let's check if the zombie list contains the cage
                    if let Some(index) = zombies.iter().position(|zombie| zombie.cageid == cageid as u64) {
                        // find the cage in zombie list, remove it from the list and break
                        zombie_opt = Some(zombies.remove(index));
                        break;
                    }

                    continue;
                }
            }
        }

        // reach here means we already found the desired exited child
        let zombie = zombie_opt.unwrap();
        // update the status
        *status = zombie.exit_code;

        // return child's cageid
        zombie.cageid as i32
    }

    pub fn wait_syscall(&self, status: &mut i32) -> i32 {
        self.waitpid_syscall(0, status, 0)
    }

    pub fn getpid_syscall(&self) -> i32 {
        self.cageid as i32 //not sure if this is quite what we want but it's easy enough to change later
    }
    pub fn getppid_syscall(&self) -> i32 {
        self.parent as i32 // mimicing the call above -- easy to change later if necessary
    }

    /*
    * if its negative 1
    * return -1, but also set the values in the cage struct to the DEFAULTs for future calls
    */
    pub fn getgid_syscall(&self) -> i32 {
        if self.getgid.load(interface::RustAtomicOrdering::Relaxed) == -1 {
            self.getgid
                .store(DEFAULT_GID as i32, interface::RustAtomicOrdering::Relaxed);
            return -1;
        }
        DEFAULT_GID as i32 //Lind is only run in one group so a default value is returned
    }
    pub fn getegid_syscall(&self) -> i32 {
        if self.getegid.load(interface::RustAtomicOrdering::Relaxed) == -1 {
            self.getegid
                .store(DEFAULT_GID as i32, interface::RustAtomicOrdering::Relaxed);
            return -1;
        }
        DEFAULT_GID as i32 //Lind is only run in one group so a default value is returned
    }

    pub fn getuid_syscall(&self) -> i32 {
        if self.getuid.load(interface::RustAtomicOrdering::Relaxed) == -1 {
            self.getuid
                .store(DEFAULT_UID as i32, interface::RustAtomicOrdering::Relaxed);
            return -1;
        }
        DEFAULT_UID as i32 //Lind is only run as one user so a default value is returned
    }
    pub fn geteuid_syscall(&self) -> i32 {
        if self.geteuid.load(interface::RustAtomicOrdering::Relaxed) == -1 {
            self.geteuid
                .store(DEFAULT_UID as i32, interface::RustAtomicOrdering::Relaxed);
            return -1;
        }
        DEFAULT_UID as i32 //Lind is only run as one user so a default value is returned
    }

    pub fn sigaction_syscall(
        &self,
        sig: i32,
        act: Option<&interface::SigactionStruct>,
        oact: Option<&mut interface::SigactionStruct>,
    ) -> i32 {
        if let Some(some_oact) = oact {
            let old_sigactionstruct = self.signalhandler.get(&sig);

            if let Some(entry) = old_sigactionstruct {
                some_oact.clone_from(entry.value());
            } else {
                some_oact.clone_from(&interface::SigactionStruct::default()); // leave handler field as NULL
            }
        }

        if let Some(some_act) = act {
            // println!("sigaction: sa_handler: {}", some_act.sa_handler);
            if sig == SIGKILL as i32 || sig == SIGSTOP as i32 {
                // Disallow changing the action for SIGKILL and SIGSTOP
                return syscall_error(
                    Errno::EINVAL,
                    "sigaction",
                    "Cannot modify the action of SIGKILL or SIGSTOP",
                );
            }

            self.signalhandler.insert(sig, some_act.clone());
        }

        0
    }

    pub fn kill_syscall(&self, cage_id: i32, sig: i32) -> i32 {
        if (cage_id < 0) || (cage_id >= interface::MAXCAGEID) {
            return syscall_error(Errno::EINVAL, "sigkill", "Invalid cage id.");
        }

        if (sig < 0) || (sig >= SIG_MAX) {
            return syscall_error(Errno::EINVAL, "sigkill", "Invalid signal number");
        }

        if let Some(cage) = interface::cagetable_getref_opt(cage_id as u64) {
            let mut pending_signals = cage.pending_signals.write();
            pending_signals.push(sig);
            // cage.pending_signals.fetch_or(1 << sig, interface::RustAtomicOrdering::SeqCst);
            // if cage.signal_triggerable.load(interface::RustAtomicOrdering::SeqCst) {
                // interface::signal_epoch_trigger(cage_id as u64);
            // }
            if !interface::signal_check_block(cage_id as u64, sig) {
                interface::signal_epoch_trigger(cage_id as u64);
            }
            0
        } else {
            return syscall_error(Errno::ESRCH, "kill", "Target cage does not exist");
        }
    }

    pub fn sigprocmask_syscall(
        &self,
        how: i32,
        set: Option<&interface::SigsetType>,
        oldset: Option<&mut interface::SigsetType>,
    ) -> i32 {
        let mut res = 0;

        if let Some(some_oldset) = oldset {
            *some_oldset = self.sigset.load(interface::RustAtomicOrdering::Relaxed);
        }

        if let Some(some_set) = set {
            let curr_sigset = self.sigset.load(interface::RustAtomicOrdering::Relaxed);
            res = match how {
                SIG_BLOCK => {
                    // Block signals in set
                    self.sigset.store(
                        curr_sigset | *some_set,
                        interface::RustAtomicOrdering::Relaxed,
                    );
                    0
                }
                SIG_UNBLOCK => {
                    // Unblock signals in set
                    let newset = curr_sigset & !*some_set;
                    self.sigset.store(newset, interface::RustAtomicOrdering::Relaxed);
                    // send pending signals
                    // TODO: check if the signal is set here is more efficient
                    // if self.signal_triggerable.load(interface::RustAtomicOrdering::SeqCst) {
                        // interface::signal_epoch_trigger(self.cageid);
                    // }
                    // let pending_signals = self.pending_signals.read();
                    // pending_signals.contains()
                    interface::signal_epoch_trigger(self.cageid);
                    0
                }
                SIG_SETMASK => {
                    // TODO: handle signal get unblocked
                    // Set sigset to set
                    self.sigset.store(*some_set, interface::RustAtomicOrdering::Relaxed);
                    0
                }
                _ => syscall_error(Errno::EINVAL, "sigprocmask", "Invalid value for how"),
            }
        }
        res
    }

    pub fn setitimer_syscall(
        &self,
        which: i32,
        new_value: Option<&interface::ITimerVal>,
        old_value: Option<&mut interface::ITimerVal>,
    ) -> i32 {
        match which {
            ITIMER_REAL => {
                if let Some(some_old_value) = old_value {
                    let (curr_duration, next_duration) = self.interval_timer.get_itimer();
                    some_old_value.it_value.tv_sec = curr_duration.as_secs() as i64;
                    some_old_value.it_value.tv_usec = curr_duration.subsec_millis() as i64;
                    some_old_value.it_interval.tv_sec = next_duration.as_secs() as i64;
                    some_old_value.it_interval.tv_usec = next_duration.subsec_millis() as i64;
                }

                if let Some(some_new_value) = new_value {
                    let curr_duration = interface::RustDuration::new(
                        some_new_value.it_value.tv_sec as u64,
                        some_new_value.it_value.tv_usec as u32,
                    );
                    let next_duration = interface::RustDuration::new(
                        some_new_value.it_interval.tv_sec as u64,
                        some_new_value.it_interval.tv_usec as u32,
                    );

                    self.interval_timer.set_itimer(curr_duration, next_duration);
                }
            }

            _ => { /* ITIMER_VIRTUAL and ITIMER_PROF is not implemented*/ }
        }
        0
    }

    pub fn getrlimit(&self, res_type: u64, rlimit: &mut interface::Rlimit) -> i32 {
        match res_type {
            RLIMIT_NOFILE => {
                rlimit.rlim_cur = NOFILE_CUR;
                rlimit.rlim_max = NOFILE_MAX;
            }
            RLIMIT_STACK => {
                rlimit.rlim_cur = STACK_CUR;
                rlimit.rlim_max = STACK_MAX;
            }
            _ => return -1,
        }
        0
    }

    pub fn setrlimit(&self, res_type: u64, _limit_value: u64) -> i32 {
        match res_type {
            RLIMIT_NOFILE => {
                if NOFILE_CUR > NOFILE_MAX {
                    -1
                } else {
                    0
                }
                //FIXME: not implemented yet to update value in program
            }
            _ => -1,
        }
    }
}
