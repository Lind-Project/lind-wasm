#![allow(dead_code)]

// System related system calls
use crate::constants::{
    DEFAULT_GID, DEFAULT_UID, ITIMER_REAL, NOFILE_CUR, NOFILE_MAX, RLIMIT_NOFILE, RLIMIT_STACK,
    SEM_VALUE_MAX, SHMMAX, SHMMIN, SHM_DEST, SHM_RDONLY, SIGCHLD, SIGNAL_MAX, SIG_BLOCK, SIG_MAX,
    SIG_SETMASK, SIG_UNBLOCK, STACK_CUR, STACK_MAX,
};

use crate::interface::{self, convert_signal_mask, lind_send_signal};
use crate::safeposix::cage;
use crate::safeposix::cage::*;
use crate::safeposix::shm::*;

use crate::fdtables;

use libc::*;

use std::io;
use std::io::Write;

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
            thread_table: interface::RustHashMap::new(),
            signalhandler: self.signalhandler.clone(),
            sigset: interface::RustAtomicU64::new(
                self.sigset.load(interface::RustAtomicOrdering::Relaxed),
            ),
            pending_signals: interface::RustLock::new(vec![]),
            epoch_handler: interface::RustHashMap::new(),
            main_threadid: interface::RustLock::new(0),
            interval_timer: interface::IntervalTimer::new(child_cageid),
            vmmap: interface::RustLock::new(new_vmmap), // clone the vmmap for the child
            zombies: interface::RustLock::new(vec![]),
            child_num: interface::RustAtomicU64::new(0),
        };

        // increment child counter for parent
        self.child_num
            .fetch_add(1, interface::RustAtomicOrdering::SeqCst);

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
     *  Here is the Linux man page for execve: https://man7.org/linux/man-pages/man2/execve.2.html
     *
     *  exec() only returns if an error occurs.
     *
     *  Unlike the `exec` syscalls in the Linux manual, the `exec` syscall here does not take any arguments,
     *  such as an argument list or environment variables. This is because, in Rawposix, `exec` functions
     *  solely as a "cage-level exec," focusing only on updating the `cage` struct with necessary changes.
     *
     *  In short, this syscall in Rawposix part is responsible for managing cage resources.
     *  Execution and memory management are handled within the Wasmtime codebase, which eventually calls
     *  this function to perform only a specific part of the `exec` operation.
     *
     *  Here, we retain the same cage and only replace the necessary components since `cageid`, `cwd`, `zombies`,
     *  and other elements remain unchanged. Only `cancelstatus`, `rev_shm`, `thread_table`, and `vmmap` need to be replaced.
     */
    pub fn exec_syscall(&self) -> i32 {
        fdtables::empty_fds_for_exec(self.cageid);

        self.cancelstatus
            .store(false, interface::RustAtomicOrdering::Relaxed);
        self.rev_shm.lock().clear();
        self.thread_table.clear();
        let mut vmmap = self.vmmap.write();
        vmmap.clear(); //this just clean the vmmap in the cage, still need some modify for wasmtime and call to kernal

        // perform signal related clean up

        // all the signal handler becomes default after exec
        // pending signals should be perserved though
        self.signalhandler.clear();
        // the sigset will be reset after exec
        self.sigset.store(0, interface::RustAtomicOrdering::Relaxed);
        // we also clean up epoch handler and main thread id
        // since they will be re-established from wasmtime
        self.epoch_handler.clear();
        let mut threadid_guard = self.main_threadid.write();
        *threadid_guard = 0;
        drop(threadid_guard);

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
                parent
                    .child_num
                    .fetch_sub(1, interface::RustAtomicOrdering::SeqCst);

                // push the exit status to parent's zombie list
                let mut zombie_vec = parent.zombies.write();
                zombie_vec.push(Zombie {
                    cageid: self.cageid,
                    exit_code: status,
                });
            } else {
                // if parent already exited
                // BUG: we currently do not handle the situation where a parent has exited already
            }
        }

        // Trigger SIGCHLD if we are currently not running rawposix test suite
        if !interface::RUSTPOSIX_TESTSUITE.load(interface::RustAtomicOrdering::Relaxed) {
            // if the cage has parent (i.e. it is not the "root" cage)
            if self.cageid != self.parent {
                lind_send_signal(self.parent, SIGCHLD);
            }
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
            return syscall_error(
                Errno::ECHILD,
                "waitpid",
                "no existing unwaited-for child processes",
            );
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
            if let Some(index) = zombies
                .iter()
                .position(|zombie| zombie.cageid == cageid as u64)
            {
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
                        return syscall_error(
                            Errno::ECHILD,
                            "waitpid",
                            "waited cage is not the child of the cage",
                        );
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
                    if let Some(index) = zombies
                        .iter()
                        .position(|zombie| zombie.cageid == cageid as u64)
                    {
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

        if !lind_send_signal(cage_id as u64, sig) {
            return syscall_error(Errno::ESRCH, "kill", "Target cage does not exist");
        }

        0
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
                    self.sigset
                        .store(newset, interface::RustAtomicOrdering::Relaxed);
                    // check if any of the unblocked signals are in the pending signal list
                    // and trigger the epoch if it has
                    let pending_signals = self.pending_signals.read();
                    if pending_signals
                        .iter()
                        .any(|signo| (*some_set & convert_signal_mask(*signo)) != 0)
                    {
                        interface::signal_epoch_trigger(self.cageid);
                    }
                    0
                }
                SIG_SETMASK => {
                    let pending_signals = self.pending_signals.read();
                    // find all signals switched from blocking to nonblocking
                    // 1. perform a xor operation to find signals that switched state
                    // all the signal masks changed from 0 to 1, or 1 to 0 are filtered in this step
                    // 2. perform an and operation to the old sigset, this further filtered masks and only
                    // left masks changed from 1 to 0
                    let unblocked_signals = (curr_sigset ^ *some_set) & curr_sigset;
                    // check if any of the unblocked signals are in the pending signal list
                    // and trigger the epoch if it has
                    if pending_signals
                        .iter()
                        .any(|signo| (unblocked_signals & convert_signal_mask(*signo)) != 0)
                    {
                        interface::signal_epoch_trigger(self.cageid);
                    }
                    // Set sigset to set
                    self.sigset
                        .store(*some_set, interface::RustAtomicOrdering::Relaxed);
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
