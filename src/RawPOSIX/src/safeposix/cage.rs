#![allow(dead_code)]
use crate::interface;
//going to get the datatypes and errnos from the cage file from now on
pub use crate::interface::errnos::{syscall_error, Errno};

pub use crate::interface::types::{
    EpollEvent, IoctlPtrUnion, PipeArray, PollStruct,
};

use super::filesystem::normpath;
pub use super::syscalls::fs_constants::*;
pub use super::syscalls::net_constants::*;
pub use super::syscalls::sys_constants::*;

pub use crate::interface::CAGE_TABLE;

#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: i32
}

#[derive(Debug)]
pub struct Cage {
    // Identifying ID number for this cage
    pub cageid: u64, 
    // Current working directory of cage, must be able to be unique from other cages
    pub cwd: interface::RustLock<interface::RustRfc<interface::RustPathBuf>>, 
    // Cage ID of parent cage
    pub parent: u64,
    // Flag used in former RustPOSIX to determine if cage needs to terminate due to fault or signal 
    // (TODO: TO BE REMOVED OR REPURPOSED)
    pub cancelstatus: interface::RustAtomicBool,
    // Identifiers for gid/uid/egid/euid 
    // (TODO: WE CAN RENAME THESE GID INSTEAD OF GETGID etc.)
    pub getgid: interface::RustAtomicI32,
    pub getuid: interface::RustAtomicI32,
    pub getegid: interface::RustAtomicI32,
    pub geteuid: interface::RustAtomicI32,
    // Reverse mapping for shared memory of addresses in cage to shmid, used for attaching and deattaching 
    // shared memory segments
    pub rev_shm: interface::Mutex<Vec<(u32, i32)>>, 
    // Old rustposix tables for handling concurrency primitives with NaCl's model 
    // (TODO: TO BE REMOVED AND REPLACED WITH TRACKING FOR FUTEXES
    pub mutex_table: interface::RustLock<Vec<Option<interface::RustRfc<interface::RawMutex>>>>,
    // Old rustposix tables for handling concurrency primitives with NaCl's model 
    // (TODO: TO BE REMOVED AND REPLACED WITH TRACKING FOR FUTEXES
    pub cv_table: interface::RustLock<Vec<Option<interface::RustRfc<interface::RawCondvar>>>>,
    // Old rustposix tables for handling concurrency primitives with NaCl's model 
    // (TODO: TO BE REMOVED AND REPLACED WITH TRACKING FOR FUTEXES
    pub sem_table: interface::RustHashMap<u32, interface::RustRfc<interface::RustSemaphore>>,
    // Table of thread IDs for all threads in this cage, formerly used for managing cage exit/destruction 
    // (TODO: TO BE REMOVED OR REPURPOSED)
    pub thread_table: interface::RustHashMap<u64, bool>,
    // Mapping of signal numbers to registered for this cage
    pub signalhandler: interface::RustHashMap<i32, interface::SigactionStruct>,
    // Set of registered signals for cage
    pub sigset: interface::RustHashMap<u64, interface::RustAtomicU64>,
    // The kernel thread id of the main thread of current cage, used because when we want to send signals, 
    // we want to send to the main thread 
    pub main_threadid: interface::RustAtomicU64,
    // Timer used for alarm() and/or setitimer()
    pub interval_timer: interface::IntervalTimer,
    // Table of child zombie entries waited on, used in wait_syscall for parents to determine whether to exit child
    pub zombies: interface::RustLock<Vec<Zombie>>,
    pub child_num: interface::RustAtomicU64
}

impl Cage {
    pub fn changedir(&self, newdir: interface::RustPathBuf) {
        let newwd = interface::RustRfc::new(normpath(newdir, self));
        let mut cwdbox = self.cwd.write();
        *cwdbox = newwd;
    }

    // function to signal all cvs in a cage when forcing exit
    pub fn signalcvs(&self) {
        let cvtable = self.cv_table.read();

        for cv_handle in 0..cvtable.len() {
            if cvtable[cv_handle as usize].is_some() {
                let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
                clonedcv.broadcast();
            }
        }
    }

    pub fn send_pending_signals(&self, sigset: interface::SigsetType, pthreadid: u64) {
        for signo in 1..SIGNAL_MAX {
            if interface::lind_sigismember(sigset, signo) {
                interface::lind_threadkill(pthreadid, signo);
            }
        }
    }
}
