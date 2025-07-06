//! This file contains all the implementation related to Cage structure. Including structure
//! definitions, a global variables that handles cage management, and cage initialization and
//! finialization required by wasmtime
use crate::memory::vmmap::*;
use fdtables;
pub use once_cell::sync::Lazy;
/// Uses spinlocks first (for short waits) and parks threads when blocking to reduce kernel
/// interaction and increases efficiency.
pub use parking_lot::RwLock;
pub use std::collections::HashMap;
use std::ffi::CString;
pub use std::path::{Path, PathBuf};
pub use std::sync::atomic::{AtomicI32, AtomicU64};
pub use std::sync::Arc;
use sysdefs::constants::err_const::VERBOSE;
use sysdefs::constants::fs_const::*;
use sysdefs::data::fs_struct::SigactionStruct;
use dashmap::DashMap;
pub use std::sync::Arc as RustRfc;


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
/// Lazy causes `CAGE_MAP` to be initialized when it is first accessed, rather than when the program starts.
// pub static CAGE_MAP: Lazy<RwLock<Vec<Option<Arc<Cage>>>>> = Lazy::new(|| {
//     let mut vec = Vec::with_capacity(MAX_CAGEID);
//     vec.resize_with(MAX_CAGEID, || None);
//     RwLock::new(vec)
// });
pub static mut CAGE_MAP: Vec<Option<RustRfc<Cage>>> = Vec::new();

pub fn check_cageid(cageid: u64) {
    if cageid >= MAXCAGEID as u64 {
        panic!("Cage ID is outside of valid range");
    }
}

/// Add a cage to `CAGE_MAP` and map `cageid` to its index
// pub fn add_cage(cageid: u64, cage: Cage) {
//     let mut list = CAGE_MAP.write();
//     if (cageid as usize) < MAX_CAGEID {
//         list[cageid as usize] = Some(Arc::new(cage));
//     } else {
//         panic!("Cage ID exceeds MAX_CAGEID: {}", cageid);
//     }
// }

pub fn add_cage(cageid: u64, cage: Cage) {
    check_cageid(cageid);
    let _insertret = unsafe { CAGE_MAP[cageid as usize].insert(RustRfc::new(cage)) };
}

/// Delete the cage from `CAGE_MAP` by `cageid` as index
// pub fn remove_cage(cageid: u64) {
//     let mut list = CAGE_MAP.write();
//     if (cageid as usize) < MAX_CAGEID {
//         list[cageid as usize] = None;
//     }
// }

pub fn remove_cage(cageid: u64) {
    check_cageid(cageid);
    unsafe { CAGE_MAP[cageid as usize].take() };
}


/// Get the cage's `Arc` reference via `cageid`
/// Error handling (when `Cage` is None) happens when calling
// pub fn get_cage(cageid: u64) -> Option<Arc<Cage>> {
//     let list = CAGE_MAP.read();
//     if (cageid as usize) < MAX_CAGEID {
//         list[cageid as usize].clone()
//     } else {
//         None
//     }
// }

pub fn cagetable_getref(cageid: u64) -> RustRfc<Cage> {
    check_cageid(cageid);
    unsafe { CAGE_MAP[cageid as usize].as_ref().unwrap().clone() }
}

pub fn cagetable_getref_opt(cageid: u64) -> Option<RustRfc<Cage>> {
    check_cageid(cageid);
    unsafe {
        match CAGE_MAP[cageid as usize].as_ref() {
            Some(cage) => Some(cage.clone()),
            None => None,
        }
    }
}

// Clear `CAGE_MAP` and exit all existing cages
//
// Return:
//     Will return a list of current cageid in CAGE_MAP, rawposix will performs exit to individual cage
// TODO: will self cageid always be same with target cageid??
// pub fn cagetable_clear() -> Vec<usize> {
//     let mut exitvec = Vec::new();

//     {
//         let mut list = CAGE_MAP.write();
//         for (cageid, cage) in list.iter_mut().enumerate() {
//             if let Some(_c) = cage.take() {
//                 exitvec.push(cageid);
//             }
//         }
//     }

//     exitvec
// }

pub fn cagetable_clear() {
    let mut exitvec = Vec::new();
    unsafe {
        for cage in CAGE_MAP.iter_mut() {
            let cageopt = cage.take();
            if cageopt.is_some() {
                exitvec.push(cageopt.unwrap());
            }
        }
    }

    for cage in exitvec {
        cage.exit_syscall(EXIT_SUCCESS);
    }
}