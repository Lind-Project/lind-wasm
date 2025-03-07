#![allow(dead_code)]
use crate::constants::{
    MAP_PRIVATE, MAP_SHARED, O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, PROT_READ, PROT_WRITE,
    SIGNAL_MAX, S_IRWXG, S_IRWXO, S_IRWXU,
};
use crate::interface;

//going to get the datatypes and errnos from the cage file from now on
pub use crate::interface::errnos::{syscall_error, Errno};

pub use crate::interface::types::{EpollEvent, IoctlPtrUnion, PipeArray, PollStruct};

use super::filesystem::normpath;
pub use super::vmmap::*;
use crate::constants::*;

pub use crate::interface::CAGE_TABLE;

#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: i32,
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
    // Table of thread IDs for all threads in this cage, formerly used for managing cage exit/destruction
    // (TODO: TO BE REMOVED OR REPURPOSED)
    pub thread_table: interface::RustHashMap<u64, bool>,
    // signalhandler is a hash map where the key is a signal number, and the value is a SigactionStruct, which
    // defines how the cage should handle a specific signal. Interacts with sigaction_syscall() to register or
    // retrieve the handler for a specific signal.
    pub signalhandler: interface::RustHashMap<i32, interface::SigactionStruct>,
    // sigset is a mapping of thread IDs (pthreadid) to atomic signal sets. Each entry represents the signals
    // currently blocked for the corresponding thread in the cage. Interacts with sigprocmask_syscall() to
    // block / unblock / replace the signal mask for a thread.
    pub sigset: interface::RustHashMap<u64, interface::RustAtomicU64>,
    // The kernel thread id of the main thread of current cage, used because when we want to send signals,
    // we want to send to the main thread
    pub main_threadid: interface::RustAtomicU64,
    // The interval_timer can serve as a source for triggering signals and works together with signalhandler
    // and sigset to manage and handle signals. The design of the interval_timer supports periodic triggering,
    // simulating operations in Linux that need to run at regular intervals. It assists in implementing setitimer()
    // in RawPOSIX, and by triggering lind_kill_from_id when the interval_timer expires
    // (implemented in src/interface/timer.rs), it facilitates the implementation of signal handling in rawposix
    // for the corresponding Cage.
    pub interval_timer: interface::IntervalTimer,
    // The zombies field in the Cage struct is used to manage information about child cages that have
    // exited, but whose exit status has not yet been retrieved by their parent using wait() / waitpid().
    // When a cage exits, shared memory segments are detached, file descriptors are removed from fdtable,
    // and cage struct is cleaned up, but its exit status are inserted along with its cage id into the end of
    // its parent cage's zombies list
    pub zombies: interface::RustLock<Vec<Zombie>>,
    pub child_num: interface::RustAtomicU64,
    pub vmmap: interface::RustLock<Vmmap>,
}

impl Cage {
    pub fn changedir(&self, newdir: interface::RustPathBuf) {
        let newwd = interface::RustRfc::new(normpath(newdir, self));
        let mut cwdbox = self.cwd.write();
        *cwdbox = newwd;
    }

    pub fn send_pending_signals(&self, sigset: interface::SigsetType, pthreadid: u64) {
        for signo in 1..SIGNAL_MAX {
            if interface::lind_sigismember(sigset, signo) {
                interface::lind_threadkill(pthreadid, signo);
            }
        }
    }
}
