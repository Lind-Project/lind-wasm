//! This file contains all the implementation related to Cage structure. Including structure
//! definitions, a global variables that handles cage management, and cage initialization and
//! finialization required by wasmtime
use crate::memory::vmmap::*;
use dashmap::DashMap;
pub use once_cell::sync::Lazy;
/// Uses spinlocks first (for short waits) and parks threads when blocking to reduce kernel
/// interaction and increases efficiency.
pub use parking_lot::{Mutex, RwLock};
pub use std::path::{Path, PathBuf};
pub use std::sync::atomic::{AtomicI32, AtomicU64};
pub use std::sync::Arc;
use sysdefs::data::fs_struct::SigactionStruct;
use sysdefs::constants::fs_const::MAX_CAGEID;

#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: i32,
}

/// I only kept required fields for cage struct
#[derive(Debug)]
pub struct Cage {
    // Identifying ID number for this cage
    pub cageid: u64,
    pub parent: u64,
    // Current working directory of cage, must be able to be unique from other cages
    pub cwd: RwLock<Arc<PathBuf>>,
    // Identifiers for gid/uid/egid/euid
    pub gid: AtomicI32,
    pub uid: AtomicI32,
    pub egid: AtomicI32,
    pub euid: AtomicI32,
    // Reverse mapping for shared memory of addresses in cage to shmid, used for attaching and deattaching
    // shared memory segments
    pub rev_shm: Mutex<Vec<(u32, i32)>>,
    // signalhandler is a hash map where the key is a signal number, and the value is a SigactionStruct, which
    // defines how the cage should handle a specific signal. Interacts with sigaction_syscall() to register or
    // retrieve the handler for a specific signal.
    pub signalhandler: DashMap<i32, SigactionStruct>,
    // sigset is an atomic signal sets representing the signals
    // currently blocked for the cage. Interacts with sigprocmask_syscall() to
    // block / unblock / replace the signal mask for a the cage.
    pub sigset: AtomicU64,
    // pending_signals are signals that are pending to be handled
    pub pending_signals: RwLock<Vec<i32>>,
    // epoch_handler is a hash map where key is the thread id of the cage, and the value is the epoch
    // address of the wasm thread. The epoch is a u64 value that guest thread is frequently checking for
    // and just to host once the value is changed
    pub epoch_handler: DashMap<i32, RwLock<*mut u64>>,
    // The kernel thread id of the main thread of current cage, used because when we want to send signals,
    // we want to send to the main thread
    pub main_threadid: RwLock<i32>,
    // The zombies field in the Cage struct is used to manage information about child cages that have
    // exited, but whose exit status has not yet been retrieved by their parent using wait() / waitpid().
    // When a cage exits, shared memory segments are detached, file descriptors are removed from fdtable,
    // and cage struct is cleaned up, but its exit status are inserted along with its cage id into the end of
    // its parent cage's zombies list
    pub zombies: RwLock<Vec<Zombie>>,
    pub child_num: AtomicU64,
    pub vmmap: RwLock<Vmmap>,
}

/// We achieve an O(1) complexity for our cage map implementation through the following three approaches:
///
/// Direct Indexing with `cageid`:
///     `cageid` directly as the index to access the `Vec`, allowing O(1) complexity for lookup, insertion,
///     and deletion.
/// `Vec<Option<Arc<Cage>>>` for Efficient Deletion:
///     When deleting an entry, we replace it with `None` instead of restructuring the `Vec`. If we were to
///     use `Vec<Arc<Cage>>`, there would be no empty slots after deletion, forcing us to use `retain()` to
///     reallocate the `Vec`, which results in O(n) complexity. Using `Vec<Option<Arc<Cage>>>` allows us to
///     maintain O(1) deletion complexity.
/// `RwLock` for Concurrent Access Control:
///     `RwLock` ensures thread-safe access to `CAGE_MAP`, providing control over concurrent reads and writes.
///     Since writes occur only during initialization (`lindrustinit`) and `fork()` / `exec()`, and deletions
///     happen only via `exit()`, the additional overhead introduced by `RwLock` should be minimal in terms
///     of overall performance impact.
///
/// Pre-allocate MAX_CAGEID elements, all initialized to None.

pub static mut CAGE_MAP: Vec<Option<Arc<Cage>>> = Vec::new();

pub fn check_cageid(cageid: u64) {
    if cageid >= MAX_CAGEID as u64 {
        panic!("Cage ID is outside of valid range");
    }
}

#[allow(static_mut_refs)]
// SAFETY: This code is single-threaded during initialization, and no other
// mutable or immutable references to `CAGE_MAP` exist while this call executes.
pub fn cagetable_init() {
    unsafe {
        for _cage in 0..MAX_CAGEID {
            CAGE_MAP.push(None);
        }
    }
}

pub fn add_cage(cageid: u64, cage: Cage) {
    check_cageid(cageid);
    let _insertret = unsafe { CAGE_MAP[cageid as usize].insert(Arc::new(cage)) };
}

pub fn remove_cage(cageid: u64) {
    check_cageid(cageid);
    unsafe { CAGE_MAP[cageid as usize].take() };
}

pub fn get_cage(cageid: u64) -> Option<Arc<Cage>> {
    check_cageid(cageid);
    unsafe {
        match CAGE_MAP[cageid as usize].as_ref() {
            Some(cage) => Some(cage.clone()),
            None => None,
        }
    }
}

#[allow(static_mut_refs)]
// SAFETY: This code is single-threaded during teardown, and no other
// mutable or immutable references to `CAGE_MAP` exist while this call executes.
pub fn cagetable_clear() -> Vec<usize> {
    let mut exitvec = Vec::new();

    unsafe {
        for (cageid, cage) in CAGE_MAP.iter_mut().enumerate() {
            let cageopt = cage.take();
            if !cageopt.is_none() {
                exitvec.push(cageid)
            }
        }
    }

    exitvec
}
