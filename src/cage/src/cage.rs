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
    // The kernel thread id of the main thread of current cage, used because when we want to send signals,
    // we want to send to the main thread
    pub main_threadid: AtomicU64,
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
pub static CAGE_MAP: Lazy<RwLock<Vec<Option<Arc<Cage>>>>> = Lazy::new(|| {
    let mut vec = Vec::with_capacity(MAX_CAGEID);
    vec.resize_with(MAX_CAGEID, || None);
    RwLock::new(vec)
});

/// Add a cage to `CAGE_MAP` and map `cageid` to its index
pub fn add_cage(cageid: u64, cage: Cage) {
    let mut list = CAGE_MAP.write();
    if (cageid as usize) < MAX_CAGEID {
        list[cageid as usize] = Some(Arc::new(cage));
    } else {
        panic!("Cage ID exceeds MAX_CAGEID: {}", cageid);
    }
}

/// Delete the cage from `CAGE_MAP` by `cageid` as index
pub fn remove_cage(cageid: u64) {
    let mut list = CAGE_MAP.write();
    if (cageid as usize) < MAX_CAGEID {
        list[cageid as usize] = None;
    }
}

/// Get the cage's `Arc` reference via `cageid`
/// Error handling (when `Cage` is None) happens when calling
pub fn get_cage(cageid: u64) -> Option<Arc<Cage>> {
    let list = CAGE_MAP.read();
    if (cageid as usize) < MAX_CAGEID {
        list[cageid as usize].clone()
    } else {
        None
    }
}

/// Clear `CAGE_MAP` and exit all existing cages
/// 
/// Return:
///     Will return a list of current cageid in CAGE_MAP, rawposix will performs exit to individual cage
/// TODO: will self cageid always be same with target cageid??
pub fn cagetable_clear() -> Vec<usize> {
    let mut exitvec = Vec::new();

    {
        let mut list = CAGE_MAP.write();
        for (cageid, cage) in list.iter_mut().enumerate() {
            if let Some(_c) = cage.take() {
                exitvec.push(cageid);
            }
        }
    }

    exitvec
}
