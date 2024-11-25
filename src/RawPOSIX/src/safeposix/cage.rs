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
pub use super::vmmap::*;
pub use super::vmmap_constants::*;

pub use crate::interface::CAGE_TABLE;

#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: i32
}

#[derive(Debug)]
pub struct Cage {
    pub cageid: u64,
    pub cwd: interface::RustLock<interface::RustRfc<interface::RustPathBuf>>,
    pub parent: u64,
    pub cancelstatus: interface::RustAtomicBool,
    pub getgid: interface::RustAtomicI32,
    pub getuid: interface::RustAtomicI32,
    pub getegid: interface::RustAtomicI32,
    pub geteuid: interface::RustAtomicI32,
    pub rev_shm: interface::Mutex<Vec<(u32, i32)>>, //maps addr within cage to shmid
    pub mutex_table: interface::RustLock<Vec<Option<interface::RustRfc<interface::RawMutex>>>>,
    pub cv_table: interface::RustLock<Vec<Option<interface::RustRfc<interface::RawCondvar>>>>,
    pub sem_table: interface::RustHashMap<u32, interface::RustRfc<interface::RustSemaphore>>,
    pub thread_table: interface::RustHashMap<u64, bool>,
    pub signalhandler: interface::RustHashMap<i32, interface::SigactionStruct>,
    pub sigset: interface::RustHashMap<u64, interface::RustAtomicU64>,
    pub pendingsigset: interface::RustHashMap<u64, interface::RustAtomicU64>,
    pub main_threadid: interface::RustAtomicU64,
    pub interval_timer: interface::IntervalTimer,
    pub zombies: interface::RustLock<Vec<Zombie>>,
    pub child_num: interface::RustAtomicU64
    pub vmmap:  Vmmap,
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
