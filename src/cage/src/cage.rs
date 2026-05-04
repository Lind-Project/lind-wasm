//! This file contains all the implementation related to Cage structure. Including structure
//! definitions, a global variables that handles cage management, and cage initialization and
//! finialization required by wasmtime
use crate::memory::vmmap::*;
use crate::timer::*;
use arc_swap::ArcSwapOption;
use dashmap::DashMap;
/// Uses spinlocks first (for short waits) and parks threads when blocking to reduce kernel
/// interaction and increases efficiency.
pub use parking_lot::{Mutex, RwLock};
pub use std::path::{Path, PathBuf};
pub use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU64, Ordering};
pub use std::sync::{Arc, LazyLock};
use sysdefs::constants::lind_platform_const::MAX_CAGEID;
use sysdefs::constants::sys_const::EXIT_SUCCESS;
use sysdefs::constants::SIGCHLD;
use sysdefs::data::fs_struct::SigactionStruct;
#[cfg(feature = "lind_debug")]
use sysdefs::logging::lind_debug_panic;

/// Represents how a cage terminated, mirroring the two primary POSIX
/// process termination modes.
///
/// A process may either:
/// - exit normally via `exit()` with an exit code, or
/// - be terminated by a signal.
///
/// This enum stores the termination information in a structured form
/// before it is encoded into the traditional POSIX wait status returned
/// by `waitpid`.
///
/// TODO: Currently, Lind-Wasm only supports normal exit and signal
/// termination. Job-control states such as `Stopped` and `Continued`
/// are not yet implemented.
#[derive(Debug, Clone, Copy)]
pub enum ExitStatus {
    /// Process exited normally with the given exit code.
    /// The exit code will later be truncated to 8 bits when encoded
    /// into a POSIX wait status.
    Exited(i32),
    /// Process was terminated by a signal.
    /// The boolean indicates whether a core dump occurred.
    Signaled(i32, bool), // (signal, core_dump)
}

/// A zombie child process.
///
/// A zombie represents a child cage that has already terminated but whose
/// termination status has not yet been collected by the parent via
/// `waitpid` or a related wait syscall.
///
/// The runtime stores the cage identifier together with the termination
/// status so the parent can later retrieve it.
#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: ExitStatus,
}

/// Encode a structured `ExitStatus` into the traditional POSIX
/// `waitpid` status integer.
///
/// The encoding follows the standard Unix wait status layout:
///
/// Normal exit:
///     status = (exit_code & 0xff) << 8
///
/// Signal termination:
///     bits 0–6   : signal number
///     bit 7      : core dump flag
///
/// Exit codes are truncated to 8 bits to match POSIX semantics.
/// This ensures that `WIFEXITED`, `WEXITSTATUS`, and related libc
/// macros behave correctly.
pub fn encode_wait_status(st: ExitStatus) -> i32 {
    match st {
        ExitStatus::Exited(code) => ((code & 0xff) << 8),
        ExitStatus::Signaled(sig, core) => {
            let mut s = sig & 0x7f;
            if core {
                s |= 0x80;
            } // core dump flag in traditional encoding
            s
        }
    }
}

/// Record the final termination status of a cage.
///
/// This function stores the exit status that will later be reported to the
/// parent when the cage becomes a zombie (e.g., via `waitpid`). The status
/// may represent either a normal exit (`Exited`) or signal-based termination
/// (`Signaled`).
///
/// The recorded status is later consumed when inserting a `Zombie` entry
/// into the parent's zombie list.
///
/// This function is currently used on signal-based termination to record
/// the signal number.
///
/// # Panics
/// Returns true if the cage exists and has been marked dead (exit_group
/// or signal termination initiated).  Returns false if the cage is alive
/// or does not exist.
pub fn is_cage_dead(cageid: u64) -> bool {
    match get_cage(cageid) {
        Some(c) => c.is_dead.load(Ordering::Acquire),
        None => false,
    }
}

///
/// Panics if the specified cage does not exist in the cage table.
pub fn cage_record_exit_status(cageid: u64, status: ExitStatus) {
    // Cage may already be removed by cage_finalize (called by the last
    // thread's OnCalledAction).  A late thread can reach exit_syscall
    // after the cage is gone if it was between futex_wake (signaling
    // pthread_join) and _exit(0) when epoch_kill_all fired — the epoch
    // doesn't take effect until the thread re-enters WASM, which may
    // not happen before the rawposix exit_syscall path runs.
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => {
            #[cfg(feature = "lind_debug")]
            lind_debug_panic(&format!(
                "cage_record_exit_status: cage {} not found",
                cageid
            ));

            return;
        }
    };
    let mut final_status = cage.final_exit_status.write();
    if final_status.is_none() {
        *final_status = Some(status);
    }
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
    // epoch_handler maps Lind thread IDs (key: i32) to raw pointers of
    // each thread's Wasmtime epoch interruption state (value:
    // AtomicPtr<u64>). It is used by epoch_kill_all during cage-wide
    // termination, such as exit_group or signal-triggered exits, to mark
    // every registered thread so that its Wasm execution observes the
    // epoch kill and exits. Each thread registers its epoch pointer when
    // entering the runtime and removes it during cleanup. This works
    // together with os_tid_map, which interrupts threads blocked in host
    // syscalls so they can re-enter Wasm and observe the epoch update.
    pub epoch_handler: DashMap<i32, AtomicPtr<u64>>,
    // os_tid_map maps Lind thread IDs (key: i32) to OS thread IDs from
    // gettid (value: i64). Used by epoch_kill_all to send SIGUSR2 to
    // threads blocked in host syscalls, interrupting them so they can
    // re-enter wasm and see the epoch kill.
    pub os_tid_map: DashMap<i32, i64>,
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
    // final_exit_status stores the terminal status of the cage once a
    // termination condition has been determined.
    //
    // This field is used as a temporary cache for the cage's final exit
    // status (either `Exited(code)` or `Signaled(signo, core_dump)`).
    // The status is recorded when the cage enters a terminal state
    // (e.g., exit syscall or signal-triggered termination), but before
    // the cage is fully cleaned up.
    //
    // The recorded value is later consumed when inserting a `Zombie`
    // entry into the parent cage's `zombies` list, which is what the
    // parent observes through `wait()` / `waitpid()`.
    //
    // This field cannot be replaced by the `exit_code` stored in
    // `Zombie`. A `Zombie` object only exists in the parent's zombie
    // list and is created during the final cleanup phase of the exiting
    // cage. However, the cage's termination reason may need to be
    // determined earlier (for example during signal handling), before
    // the zombie entry is created. Therefore, the cage must temporarily
    // store its final termination status until the zombie entry is
    // generated.
    pub final_exit_status: RwLock<Option<ExitStatus>>,
    // Atomic flag to ensure only one thread wins the exit_group race.
    // When multiple threads call exit_syscall simultaneously, only the
    // first one (CAS false→true) does epoch_kill_all + wait. Others
    // just clean up their own thread and return.
    pub exit_group_initiated: AtomicBool,
    /// Set to true when the cage enters a terminal state (exit_group or
    /// signal termination).  Checked by make_syscall so that
    /// grate-forwarded calls to this cage return -ESRCH immediately
    /// instead of reaching rawposix.  The cage struct remains in
    /// CAGE_MAP until the actual last thread exits.
    ///
    /// Note: this is NOT redundant with EXITING_TABLE in threei.
    /// is_dead is a fast atomic on the Cage struct, available while
    /// the cage still exists in CAGE_MAP.  EXITING_TABLE persists
    /// after remove_cage() deletes the cage from CAGE_MAP, catching
    /// calls where is_cage_dead() would return false (cage gone, not
    /// "dead").  is_dead is also used by the grate_inflight
    /// double-check which needs an atomic on the cage struct.
    /// TODO: evaluate whether we can consolidate is_dead and
    /// EXITING_TABLE into a single mechanism.
    pub is_dead: AtomicBool,
    /// Number of in-flight grate dispatches executing on this cage's
    /// backup VMContexts.  Incremented before _call_grate_func,
    /// decremented after it returns.  cage_finalize() spins until this
    /// reaches 0 to avoid removing a cage while a grate call is still
    /// accessing it.
    ///
    /// TODO: add a mechanism to actively kill running grate instances
    /// (backup VMContexts) when the main instance exits, rather than
    /// just waiting for in-flight calls to drain.  This would likely
    /// require epoch-based interruption of backup VMContext instances.
    /// Could also be moved to wasmtime/crate/lind-3i if grate_inflight
    /// tracking is considered a VMContext-level concern.
    pub grate_inflight: AtomicU64,
}

/// Global cage table indexed by cage ID.
///
/// Each slot stores an optional `Arc<Cage>` using `ArcSwapOption`, allowing
/// readers to load cage references concurrently without taking a lock.
///
/// Empty slots represent unused or finalized cage IDs. A cage is inserted
/// with `add_cage()` and removed with `remove_cage()` during final teardown.
///
/// The table is lazily initialized on first use and contains one slot for
/// every valid cage ID in `0..MAX_CAGEID`.
pub static CAGE_MAP: LazyLock<Vec<ArcSwapOption<Cage>>> =
    LazyLock::new(|| (0..MAX_CAGEID).map(|_| ArcSwapOption::empty()).collect());

pub fn check_cageid(cageid: u64) {
    if cageid >= MAX_CAGEID as u64 {
        panic!("Cage ID is outside of valid range");
    }
}

pub fn cagetable_init() {
    LazyLock::force(&CAGE_MAP);
}

pub fn add_cage(cageid: u64, cage: Cage) {
    check_cageid(cageid);

    CAGE_MAP[cageid as usize].store(Some(Arc::new(cage)));
}

pub fn remove_cage(cageid: u64) {
    check_cageid(cageid);

    CAGE_MAP[cageid as usize].store(None);
}

pub fn get_cage(cageid: u64) -> Option<Arc<Cage>> {
    if cageid >= MAX_CAGEID as u64 {
        return None;
    }

    CAGE_MAP[cageid as usize].load_full()
}

/// Borrows the `cageid` cage and applies a function `f` to it, return `Some(R)` or `None`
/// depending on whether tha cage is out of range or does not exist.
///
/// Preferred over `get_cage` for synchronous operations where cloning the cage through an `Arc` is
/// unnecessary.
///
/// SAFETY: Assumes that the cage cannot be removed while `f` is running (e.g. by ensuring
/// `grate_inflight > 0`).
pub fn with_cage<F, R>(cageid: u64, f: F) -> Option<R>
where
    F: FnOnce(&Cage) -> R,
{
    if cageid >= MAX_CAGEID as u64 {
        return None;
    }

    let guard = CAGE_MAP[cageid as usize].load();

    guard.as_deref().map(f)
}

// SAFETY: This code is single-threaded during teardown, and no other
// mutable or immutable references to `CAGE_MAP` exist while this call executes.
pub fn cagetable_clear() -> Vec<usize> {
    let mut exitvec = Vec::new();

    for (cageid, slot) in CAGE_MAP.iter().enumerate() {
        let old = slot.swap(None);
        if old.is_some() {
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
        (v + 1 < MAX_CAGEID as u64).then_some(v + 1)
    }) {
        Ok(v) => Some(v + 1),
        Err(_) => None,
    }
}

/// Final cage teardown.  Called from exit_call's OnCalledAction when
/// the actual last thread finishes its asyncify unwind.
///
/// 1. Spins until `grate_inflight` reaches 0 (all grate dispatches on
///    backup VMContexts have returned).
/// 2. Records a zombie entry in the parent cage and sends SIGCHLD so
///    waitpid() in the parent unblocks.
/// 3. Removes the cage from the fd table and global cage table.
pub fn cage_finalize(cageid: u64) {
    if let Some(cage) = get_cage(cageid) {
        // Wait for all in-flight grate dispatches to drain.
        while cage.grate_inflight.load(Ordering::Acquire) > 0 {
            std::hint::spin_loop();
        }

        // Record zombie and notify parent.
        if cage.parent != cageid {
            if let Some(parent) = get_cage(cage.parent) {
                parent.child_num.fetch_sub(1, Ordering::SeqCst);
                let mut zombie_vec = parent.zombies.write();
                let zombie_status = {
                    let recorded = *cage.final_exit_status.read();
                    recorded.unwrap_or(ExitStatus::Exited(EXIT_SUCCESS))
                };
                zombie_vec.push(Zombie {
                    cageid,
                    exit_code: zombie_status,
                });
            }
            crate::signal::signal::lind_send_signal(cage.parent, SIGCHLD);
        }
    }

    fdtables::remove_cage_from_fdtable(cageid);
    remove_cage(cageid);
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
            os_tid_map: DashMap::new(),
            main_threadid: RwLock::new(0),
            interval_timer: crate::timer::IntervalTimer::new(2),
            zombies: RwLock::new(vec![]),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(crate::memory::vmmap::Vmmap::new()),
            final_exit_status: RwLock::new(None),
            exit_group_initiated: AtomicBool::new(false),
            is_dead: AtomicBool::new(false),
            grate_inflight: AtomicU64::new(0),
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
