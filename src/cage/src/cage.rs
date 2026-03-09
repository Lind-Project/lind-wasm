//! This file contains all the implementation related to Cage structure. Including structure
//! definitions, a global variables that handles cage management, and cage initialization and
//! finialization required by wasmtime
use crate::memory::vmmap::*;
use crate::timer::*;
use dashmap::DashMap;
pub use once_cell::sync::Lazy;
/// Uses spinlocks first (for short waits) and parks threads when blocking to reduce kernel
/// interaction and increases efficiency.
pub use parking_lot::{Mutex, RwLock};
pub use std::path::{Path, PathBuf};
pub use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
pub use std::sync::Arc;
use sysdefs::constants::lind_platform_const::MAX_CAGEID;
use sysdefs::data::fs_struct::SigactionStruct;

#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: i32,
}

#[derive(Debug)]
pub struct Cage {
    // Identifying ID number for this cage
    pub cageid: u64,
    // parent stores the cage ID of the parent cage that created the current cage.
    // This hierarchical relationship enables process-like lineage tracking, allowing
    // operations such as wait(), signal propagation, and cleanup delegation to follow
    // parent-child relationships between cages. It functions similarly to a parent PID
    // in traditional operating systems.
    pub parent: u64,
    // Current working directory of cage, must be able to be unique from other cages
    pub cwd: RwLock<Arc<PathBuf>>,
    // Reverse mapping for shared memory of addresses in cage to shmid, used for attaching and deattaching
    // shared memory segments
    pub rev_shm: Mutex<Vec<(u64, i32)>>,
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
    // The interval_timer can serve as a source for triggering signals and works together with signalhandler
    // and sigset to manage and handle signals. The design of the interval_timer supports periodic triggering,
    // simulating operations in Linux that need to run at regular intervals. It assists in implementing setitimer()
    // in RawPOSIX, and by triggering lind_kill_from_id when the interval_timer expires
    // (implemented in src/interface/timer.rs), it facilitates the implementation of signal handling in rawposix
    // for the corresponding Cage.
    pub interval_timer: IntervalTimer,
    // The zombies field in the Cage struct is used to manage information about child cages that have
    // exited, but whose exit status has not yet been retrieved by their parent using wait() / waitpid().
    // When a cage exits, shared memory segments are detached, file descriptors are removed from fdtable,
    // and cage struct is cleaned up, but its exit status are inserted along with its cage id into the end of
    // its parent cage's zombies list
    pub zombies: RwLock<Vec<Zombie>>,
    // child_num keeps track of the number of active child cages created by the current cage.
    // It is incremented when a new child cage is spawned (e.g., during `fork` or `clone` operations)
    // and decremented when a child cage exits. This field helps manage synchronization and
    // cleanup, and supports wait-related system calls for determining when all children have
    // terminated.
    pub child_num: AtomicU64,
    // vmmap represents the virtual memory mapping for this cage. More details on `memory::vmmap`
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

static CAGE_MAP: Lazy<Vec<Mutex<Option<Arc<Cage>>>>> = Lazy::new(|| {
    let mut v = Vec::with_capacity(MAX_CAGEID);
    for _ in 0..MAX_CAGEID {
        v.push(Mutex::new(None));
    }
    v
});

pub fn check_cageid(cageid: u64) {
    if cageid >= MAX_CAGEID as u64 {
        panic!("Cage ID is outside of valid range");
    }
}

pub fn cagetable_init() {
    // Force Lazy initialization. The actual allocation happens in the Lazy closure.
    let _ = CAGE_MAP.len();
}

pub fn add_cage(cageid: u64, cage: Cage) {
    check_cageid(cageid);
    CAGE_MAP[cageid as usize].lock().replace(Arc::new(cage));
}

pub fn remove_cage(cageid: u64) {
    check_cageid(cageid);
    CAGE_MAP[cageid as usize].lock().take();
}

pub fn get_cage(cageid: u64) -> Option<Arc<Cage>> {
    if cageid >= MAX_CAGEID as u64 {
        return None;
    }
    CAGE_MAP[cageid as usize].lock().as_ref().cloned()
}

pub fn cagetable_clear() -> Vec<usize> {
    let mut exitvec = Vec::new();
    for (cageid, slot) in CAGE_MAP.iter().enumerate() {
        if slot.lock().take().is_some() {
            exitvec.push(cageid);
        }
    }
    exitvec
}

/// Global cage ID allocator shared across all cages and subsystems.
///
/// This allocator exists because cage IDs cannot be derived from the
/// current cage's ID (e.g., `current_id + 1`).  Forking does not
/// guarantee that the parent cage's numeric ID is the latest assigned:
///
/// Example:
///    - Cage 10 exists
///    - Other subsystem creates Cage 11
///    - Cage 10 now calls fork()
///
/// In this situation, the next available cage ID must be 12, not 11.
/// Therefore, we must maintain a globally monotonic counter that tracks
/// the highest cage ID ever assigned, independent of which cage performs
/// the fork.
///
/// `AtomicU64::fetch_update` ensures unique, monotonic, thread-safe allocation.
static NEXT_CAGEID: AtomicU64 = AtomicU64::new(1);

/// Allocate the next available cage ID.
///
/// Returns `Some(id)` on success, or `None` if the ID space has been exhausted.
/// The returned `id` is guaranteed to be strictly greater than any previously
/// allocated ID, even under concurrent calls.
pub fn alloc_cage_id() -> Option<u64> {
    match NEXT_CAGEID.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
        (v <= MAX_CAGEID as u64).then_some(v + 1)
    }) {
        Ok(v) => Some(v + 1),
        Err(_) => None,
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_get_cage_out_of_range() {
        cagetable_init();
        let larger_cage_id = 9999999;
        let result = get_cage(larger_cage_id);
        assert! {
            result.is_none(),
            "get_cage should return none when cage_id >= MAX_CAGE_ID"
        };

        // test with max u64 value
        let max_cage_id = u64::MAX;
        let result = get_cage(max_cage_id);
        assert! {
            result.is_none(),
            "get_cage should return none when cage_id >= MAX_CAGE_ID"
        };
    }

    #[test]
    fn test_get_cage_valid() {
        cagetable_init();
        // Create a cage with ID 2
        let test_cage = Cage {
            cageid: 2,
            parent: 1,
            cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
            rev_shm: Mutex::new(Vec::new()),
            signalhandler: DashMap::new(),
            sigset: AtomicU64::new(0),
            pending_signals: RwLock::new(vec![]),
            epoch_handler: DashMap::new(),
            main_threadid: RwLock::new(0),
            interval_timer: crate::timer::IntervalTimer::new(2),
            zombies: RwLock::new(vec![]),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(crate::memory::vmmap::Vmmap::new()),
        };

        add_cage(2, test_cage);

        let result = get_cage(2);
        assert_eq!(
            result.unwrap().cageid,
            2,
            "Retrieved cage should have correct ID"
        );
    }
}
