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
use sysdefs::lind_log;

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
            lind_log!("cage_record_exit_status: cage {} not found", cageid);

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

#[cfg(test)]
mod tests {
    use super::*;
    use fdtables::FDTableEntry;
    use std::ops::Range;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Barrier;
    use std::thread;

    // ----------------------------------------------------------------------
    // Shared test infrastructure
    // ----------------------------------------------------------------------

    /// Serializes every test in this module that mutates global cage / fdtable
    /// state (`CAGE_MAP`, the fdtables global tables). A `parking_lot::Mutex` is
    /// used rather than `std::sync::Mutex` because it does not poison: a
    /// panicking test then fails on its own instead of turning every later test
    /// into a confusing `PoisonError`. Dirty state left behind by a panicking
    /// test is instead caught by each test's own clean-slate preconditions.
    static CAGE_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn cage_test_guard() -> parking_lot::MutexGuard<'static, ()> {
        CAGE_TEST_LOCK.lock()
    }

    /// Constructs a `Cage` for tests. `Cage` has no `Default` impl or
    /// constructor, so every call site (production and test) writes out all
    /// fields longhand; this centralizes that for the test module.
    fn make_test_cage(cageid: u64, parent: u64) -> Cage {
        Cage {
            cageid,
            parent,
            cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
            rev_shm: Mutex::new(Vec::new()),
            // Empty signalhandler/epoch_handler/os_tid_map keep SIGCHLD on its
            // default (Ignore) disposition, so cage_finalize's
            // lind_send_signal(parent, SIGCHLD) returns early without touching
            // pending_signals, the epoch mechanism, or tkill.
            signalhandler: DashMap::new(),
            sigset: AtomicU64::new(0),
            pending_signals: RwLock::new(vec![]),
            epoch_handler: DashMap::new(),
            os_tid_map: DashMap::new(),
            main_threadid: RwLock::new(0),
            interval_timer: crate::timer::IntervalTimer::new(cageid),
            zombies: RwLock::new(vec![]),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(crate::memory::vmmap::Vmmap::new()),
            final_exit_status: RwLock::new(None),
            exit_group_initiated: AtomicBool::new(false),
            is_dead: AtomicBool::new(false),
            grate_inflight: AtomicU64::new(0),
        }
    }

    #[test]
    fn test_get_cage_out_of_range() {
        let _guard = cage_test_guard();
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
        let _guard = cage_test_guard();
        cagetable_init();

        assert!(
            get_cage(2).is_none(),
            "cage 2 leaked in from another test — check for a missing cleanup"
        );

        add_cage(2, make_test_cage(2, 1));

        let result = get_cage(2);
        assert_eq!(
            result.unwrap().cageid,
            2,
            "Retrieved cage should have correct ID"
        );

        remove_cage(2);
        assert!(get_cage(2).is_none());
    }

    // ----------------------------------------------------------------------
    // CONC-001 — Cage spawn/destroy stress
    //
    // Verifies: no panic/deadlock/stale CAGE_MAP entry; an `Arc<Cage>`
    // obtained before removal stays valid until released; fd-table
    // resources are released exactly once; repeated create/destroy does
    // not leak state across iterations.
    // ----------------------------------------------------------------------

    // Reserved cage-id block for this test file: far from INIT_CAGEID (1) and
    // from the id used by test_get_cage_valid (2); comfortably below
    // MAX_CAGEID (2048, enforced by check_cageid) so add_cage/remove_cage never
    // panic; and never touched via alloc_cage_id() (whose private, monotonic
    // counter starts at 1 and is never reset). 2016..2047 is left free for
    // other CONC/ISO test rows.
    const PARENT_ID: u64 = 2000;
    const CHILD_BASE: u64 = 2001;
    const CHILD_SLOTS: u64 = 8;
    const GRATE_ID: u64 = 2010;
    const RESERVED: Range<u64> = 2000..2016;

    // fdtables bookkeeping is keyed by (fdkind, underfd), so a dedicated fdkind
    // plus a disjoint per-iteration underfd window (see FDS_PER_ITER) guarantee
    // no key ever aliases across iterations or with production fdkinds.
    const TEST_FDKIND: u32 = 0x7E57_0001;
    const UNDERFD_BASE: u64 = 0x1000_0000;
    const FDS_PER_ITER: u64 = 8;

    static LAST_CLOSES: AtomicUsize = AtomicUsize::new(0);
    static INTERMEDIATE_CLOSES: AtomicUsize = AtomicUsize::new(0);
    static HANDLER_ERRORS: AtomicUsize = AtomicUsize::new(0);
    static RELEASED: LazyLock<Mutex<Vec<u64>>> = LazyLock::new(|| Mutex::new(Vec::new()));
    static INTERMEDIATES: LazyLock<Mutex<Vec<(u64, u64)>>> =
        LazyLock::new(|| Mutex::new(Vec::new()));
    static HANDLERS_ONCE: std::sync::Once = std::sync::Once::new();

    // `register_close_handlers` takes plain `fn` pointers (they cannot capture
    // state), so all bookkeeping lives in the statics above. Handlers never
    // panic themselves — a panic here would surface deep inside fdtables'
    // teardown path and be very hard to attribute; instead they record a
    // mismatch into HANDLER_ERRORS for the test to assert on.
    fn test_last_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != TEST_FDKIND || remaining != 0 {
            HANDLER_ERRORS.fetch_add(1, Ordering::SeqCst);
        }
        RELEASED.lock().push(entry.underfd);
        LAST_CLOSES.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn test_intermediate_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != TEST_FDKIND || remaining == 0 {
            HANDLER_ERRORS.fetch_add(1, Ordering::SeqCst);
        }
        INTERMEDIATES.lock().push((entry.underfd, remaining));
        INTERMEDIATE_CLOSES.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn register_test_close_handlers() {
        HANDLERS_ONCE.call_once(|| {
            fdtables::register_close_handlers(
                TEST_FDKIND,
                test_intermediate_close,
                test_last_close,
            );
        });
    }

    fn assert_reserved_ids_clean() {
        for id in RESERVED {
            assert!(
                get_cage(id).is_none(),
                "cage {id} leaked into CONC-001 tests — a previous test did not clean up"
            );
            assert!(
                !fdtables::check_cage_exists(id),
                "fdtable for cage {id} leaked into CONC-001 tests"
            );
        }
    }

    const READERS: usize = 6;
    const GUARD_SPIN: u32 = 64;
    const SPIN_BUDGET: u64 = 5_000_000;

    /// Per-round shared state between the persistent reader/destroyer pool and
    /// the main thread. Reused across every round via the two barriers so the
    /// test does not pay ~600 * 7 thread-spawn costs.
    struct Round {
        child: AtomicU64,
        saw_live: AtomicUsize,
        saw_gone: AtomicUsize,
        reader_errors: AtomicUsize,
        reader_stalls: AtomicUsize,
        destroy_errors: AtomicUsize,
        start: Barrier,
        end: Barrier,
        shutdown: AtomicBool,
    }

    // Worker bodies must never `assert!` — a panic here would leave `Barrier`
    // permanently short one party and deadlock every later round. Failures are
    // instead recorded into the Round's atomics; only the main thread asserts.
    fn reader_body(round: &Round) {
        let child = round.child.load(Ordering::Acquire);
        let mut seen_live = false;
        let mut seen_gone = false;
        let mut budget = SPIN_BUDGET;

        while !(seen_live && seen_gone) {
            if budget == 0 {
                round.reader_stalls.fetch_add(1, Ordering::SeqCst);
                return;
            }
            budget -= 1;

            // Owning path: get_cage() / load_full().
            match get_cage(child) {
                Some(c) => {
                    // A torn or aliased slot would show up as a foreign cage here.
                    if c.cageid != child || c.parent != PARENT_ID {
                        round.reader_errors.fetch_add(1, Ordering::SeqCst);
                    }
                    if !seen_live {
                        seen_live = true;
                        round.saw_live.fetch_add(1, Ordering::SeqCst);
                    }
                    let _ = c.child_num.load(Ordering::Relaxed);
                }
                None => {
                    if seen_live {
                        if !seen_gone {
                            seen_gone = true;
                            round.saw_gone.fetch_add(1, Ordering::SeqCst);
                        }
                    } else {
                        // Impossible by construction: the destroyer will not
                        // finalize until every reader has observed the cage
                        // live, so a reader may never see removal first.
                        round.reader_errors.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }

            // Borrowing path: with_cage() holds the arc-swap Guard across a
            // self-contained burst, so remove_cage()'s store(None) is likely to
            // land inside the closure's lifetime. The closure must never wait
            // on another thread, and must never take a lock owned by the
            // parent cage (cage_finalize holds parent.zombies.write()).
            let ok = with_cage(child, |c| {
                let id = c.cageid;
                let p = c.parent;
                let has_cwd = !(**c.cwd.read()).as_os_str().is_empty();
                for _ in 0..GUARD_SPIN {
                    std::hint::spin_loop();
                }
                id == child && p == PARENT_ID && has_cwd
            });
            if ok == Some(false) {
                round.reader_errors.fetch_add(1, Ordering::SeqCst);
            }

            // Readers never call any fdtables API: nearly every one of them
            // asserts the cage exists and panics otherwise, and the destroyer
            // may remove this cage's fd table at any moment.
        }
    }

    fn destroy_body(round: &Round) {
        let child = round.child.load(Ordering::Acquire);
        let mut budget = SPIN_BUDGET;
        while round.saw_live.load(Ordering::Acquire) < READERS {
            if budget == 0 {
                round.destroy_errors.fetch_add(1, Ordering::SeqCst);
                break;
            }
            budget -= 1;
            std::hint::spin_loop();
        }
        // Mirror the real exit path (see execute.rs) before tearing down.
        with_cage(child, |c| c.is_dead.store(true, Ordering::Release));
        cage_finalize(child);
    }

    fn spawn_workers(round: Arc<Round>) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::with_capacity(READERS + 1);
        for _ in 0..READERS {
            let r = Arc::clone(&round);
            handles.push(thread::spawn(move || loop {
                r.start.wait();
                if r.shutdown.load(Ordering::Acquire) {
                    break;
                }
                reader_body(&r);
                r.end.wait();
            }));
        }
        let r = Arc::clone(&round);
        handles.push(thread::spawn(move || loop {
            r.start.wait();
            if r.shutdown.load(Ordering::Acquire) {
                break;
            }
            destroy_body(&r);
            r.end.wait();
        }));
        handles
    }

    /// Runs one create -> concurrent-access -> destroy round for `child`, and
    /// asserts every CONC-001 oracle. `iter` is a globally monotonic counter
    /// (not reset between phases) used to keep every underfd unique.
    fn run_one_round(round: &Round, parent: &Cage, child: u64, iter: u64) {
        assert!(
            get_cage(child).is_none(),
            "iter {iter}: stale CAGE_MAP entry for cage {child} from a previous round"
        );
        assert!(
            !fdtables::check_cage_exists(child),
            "iter {iter}: stale fdtable for cage {child} from a previous round"
        );

        add_cage(child, make_test_cage(child, PARENT_ID));
        // Mirror fork_syscall's `selfcage.child_num.fetch_add(1, SeqCst)`:
        // without this, cage_finalize's `parent.child_num.fetch_sub(1, SeqCst)`
        // wraps an AtomicU64 with value 0 around to u64::MAX.
        parent.child_num.fetch_add(1, Ordering::SeqCst);

        fdtables::init_empty_cage(child);

        let base = UNDERFD_BASE + iter * FDS_PER_ITER;
        let (u_a, u_b, u_c, u_d, u_s) = (base, base + 1, base + 2, base + 3, base + 4);

        // Four release shapes in one round: two plain fds, a dup'd underfd
        // held twice within the same cage, an fd closed explicitly before
        // teardown, and an underfd shared with the parent cage.
        fdtables::get_unused_virtual_fd(child, TEST_FDKIND, u_a, false, 0).unwrap();
        fdtables::get_unused_virtual_fd(child, TEST_FDKIND, u_b, false, 0).unwrap();
        fdtables::get_unused_virtual_fd(child, TEST_FDKIND, u_c, false, 0).unwrap(); // refcount 1
        fdtables::get_unused_virtual_fd(child, TEST_FDKIND, u_c, false, 0).unwrap(); // dup: refcount 2
        let fd_d = fdtables::get_unused_virtual_fd(child, TEST_FDKIND, u_d, false, 0).unwrap();
        fdtables::get_unused_virtual_fd(child, TEST_FDKIND, u_s, false, 0).unwrap();
        let parent_fd_s =
            fdtables::get_unused_virtual_fd(PARENT_ID, TEST_FDKIND, u_s, false, 0).unwrap();

        // Explicit close before teardown proves u_d is released here, and
        // never again when the cage is removed (the row slot is already empty).
        let last_before = LAST_CLOSES.load(Ordering::SeqCst);
        fdtables::close_virtualfd(child, fd_d).unwrap();
        assert_eq!(
            LAST_CLOSES.load(Ordering::SeqCst),
            last_before + 1,
            "iter {iter}: explicit close of underfd {u_d} did not fire the last-close handler"
        );

        let exit_code = ExitStatus::Exited((iter % 200) as i32);
        cage_record_exit_status(child, exit_code);

        let retained = get_cage(child).expect("cage vanished before the round started");
        assert_eq!(retained.cageid, child);
        assert_eq!(retained.parent, PARENT_ID);

        RELEASED.lock().clear();
        INTERMEDIATES.lock().clear();
        let (last0, mid0) = (
            LAST_CLOSES.load(Ordering::SeqCst),
            INTERMEDIATE_CLOSES.load(Ordering::SeqCst),
        );

        round.child.store(child, Ordering::Release);
        round.saw_live.store(0, Ordering::Release);
        round.saw_gone.store(0, Ordering::Release);
        round.reader_errors.store(0, Ordering::SeqCst);
        round.reader_stalls.store(0, Ordering::SeqCst);
        round.destroy_errors.store(0, Ordering::SeqCst);

        round.start.wait();
        round.end.wait();

        assert_eq!(
            round.reader_errors.load(Ordering::SeqCst),
            0,
            "iter {iter}: a reader observed a torn/foreign cage or bad ordering"
        );
        assert_eq!(
            round.reader_stalls.load(Ordering::SeqCst),
            0,
            "iter {iter}: a reader thread exhausted its spin budget (possible hang)"
        );
        assert_eq!(
            round.destroy_errors.load(Ordering::SeqCst),
            0,
            "iter {iter}: the destroyer thread exhausted its spin budget (possible hang)"
        );
        assert_eq!(
            round.saw_live.load(Ordering::SeqCst),
            READERS,
            "iter {iter}: not every reader observed the live cage"
        );
        assert_eq!(
            round.saw_gone.load(Ordering::SeqCst),
            READERS,
            "iter {iter}: not every reader observed the cage's removal"
        );

        // --- No stale CAGE_MAP entry ---
        assert!(
            get_cage(child).is_none(),
            "iter {iter}: cage {child} still present in CAGE_MAP after finalize"
        );
        assert!(
            with_cage(child, |_| ()).is_none(),
            "iter {iter}: with_cage disagrees with get_cage on cage {child}"
        );
        assert!(
            !fdtables::check_cage_exists(child),
            "iter {iter}: fdtable for cage {child} was not removed"
        );

        // --- The Arc taken before removal remains fully valid ---
        assert_eq!(retained.cageid, child);
        assert_eq!(retained.parent, PARENT_ID);
        assert!(
            retained.is_dead.load(Ordering::SeqCst),
            "iter {iter}: is_dead not observed on the retained Arc<Cage>"
        );
        assert_eq!((**retained.cwd.read()).clone(), PathBuf::from("/"));
        assert!(
            retained.zombies.read().is_empty(),
            "iter {iter}: a child cage should never have its own zombies"
        );

        // --- Parent-side bookkeeping: exactly one decrement, one zombie ---
        assert_eq!(
            parent.child_num.load(Ordering::SeqCst),
            0,
            "iter {iter}: parent.child_num did not return to 0 (underflow or missed decrement)"
        );
        {
            let zombies = parent.zombies.read();
            let z = zombies.last().expect("no zombie recorded for this round");
            assert_eq!(z.cageid, child, "iter {iter}: zombie has the wrong cageid");
            assert_eq!(
                encode_wait_status(z.exit_code),
                encode_wait_status(exit_code),
                "iter {iter}: zombie has the wrong exit status"
            );
        }
        assert!(
            parent.pending_signals.read().is_empty(),
            "iter {iter}: SIGCHLD was unexpectedly queued (did the Ignore-default assumption change?)"
        );

        // --- Close-handler accounting: released exactly once ---
        assert_eq!(
            LAST_CLOSES.load(Ordering::SeqCst) - last0,
            3,
            "iter {iter}: unexpected number of last-close events at teardown"
        );
        assert_eq!(
            INTERMEDIATE_CLOSES.load(Ordering::SeqCst) - mid0,
            2,
            "iter {iter}: unexpected number of intermediate-close events at teardown"
        );
        {
            let mut released = RELEASED.lock().clone();
            released.sort_unstable();
            assert_eq!(
                released,
                vec![u_a, u_b, u_c],
                "iter {iter}: wrong set of underfds released at teardown"
            );
        }
        {
            let mut intermediates = INTERMEDIATES.lock().clone();
            intermediates.sort_unstable();
            assert_eq!(
                intermediates,
                vec![(u_c, 1), (u_s, 1)],
                "iter {iter}: wrong intermediate-close events at teardown"
            );
        }
        assert_eq!(
            HANDLER_ERRORS.load(Ordering::SeqCst),
            0,
            "iter {iter}: a close handler observed the wrong fdkind or refcount polarity"
        );

        // Cross-cage exactly-once: the shared underfd is released only when
        // the parent, its last remaining holder, closes it, never at the
        // child's teardown.
        let last_before_shared = LAST_CLOSES.load(Ordering::SeqCst);
        fdtables::close_virtualfd(PARENT_ID, parent_fd_s).unwrap();
        assert_eq!(
            LAST_CLOSES.load(Ordering::SeqCst),
            last_before_shared + 1,
            "iter {iter}: shared underfd {u_s} not released when the parent closed it"
        );
        assert!(
            RELEASED.lock().contains(&u_s),
            "iter {iter}: shared underfd {u_s} missing from the release list"
        );

        drop(retained);
    }

    #[test]
    fn conc_001_cage_spawn_destroy_stress() {
        let _guard = cage_test_guard();
        cagetable_init();
        register_test_close_handlers();
        assert_reserved_ids_clean();

        add_cage(PARENT_ID, make_test_cage(PARENT_ID, PARENT_ID)); // self-parented
        fdtables::init_empty_cage(PARENT_ID);
        let parent = get_cage(PARENT_ID).expect("parent cage just inserted");

        let round = Arc::new(Round {
            child: AtomicU64::new(0),
            saw_live: AtomicUsize::new(0),
            saw_gone: AtomicUsize::new(0),
            reader_errors: AtomicUsize::new(0),
            reader_stalls: AtomicUsize::new(0),
            destroy_errors: AtomicUsize::new(0),
            start: Barrier::new(READERS + 2), // READERS + destroyer + main
            end: Barrier::new(READERS + 2),
            shutdown: AtomicBool::new(false),
        });
        let handles = spawn_workers(Arc::clone(&round));

        let iters: usize = std::env::var("LIND_CONC001_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);

        let mut global_iter: u64 = 0;
        let mut total_children: u64 = 0;

        // Phase A: rotate over CHILD_SLOTS distinct ids (slot reuse at period 8).
        for i in 0..iters {
            let child = CHILD_BASE + (i as u64 % CHILD_SLOTS);
            run_one_round(&round, &parent, child, global_iter);
            global_iter += 1;
            total_children += 1;
        }
        // Phase B: pin a single id so add -> destroy -> add lands back-to-back
        // in the same ArcSwapOption slot, the sharpest test of "no state
        // leaks into later iterations".
        for _ in 0..(iters / 5) {
            run_one_round(&round, &parent, CHILD_BASE, global_iter);
            global_iter += 1;
            total_children += 1;
        }

        // Cumulative oracles across every round.
        assert_eq!(
            parent.child_num.load(Ordering::SeqCst),
            0,
            "parent.child_num did not end at 0"
        );
        assert_eq!(
            parent.zombies.read().len() as u64,
            total_children,
            "zombie count does not match the number of destroyed children"
        );
        assert!(
            parent.pending_signals.read().is_empty(),
            "SIGCHLD unexpectedly queued on the parent"
        );
        assert_eq!(
            HANDLER_ERRORS.load(Ordering::SeqCst),
            0,
            "a close handler observed the wrong fdkind or refcount polarity at some point"
        );

        round.shutdown.store(true, Ordering::Release);
        round.start.wait();
        for h in handles {
            h.join().expect("a worker thread panicked");
        }

        fdtables::remove_cage_from_fdtable(PARENT_ID);
        remove_cage(PARENT_ID);
        assert_reserved_ids_clean();
    }

    /// Covers the one `cage_finalize` path the stress test above cannot reach:
    /// it always runs with `grate_inflight == 0`, so the drain spin is a no-op.
    #[test]
    fn conc_001_finalize_waits_for_grate_inflight() {
        let _guard = cage_test_guard();
        cagetable_init();
        register_test_close_handlers();

        let id = GRATE_ID;
        assert!(get_cage(id).is_none(), "cage {id} leaked from another test");
        assert!(
            !fdtables::check_cage_exists(id),
            "fdtable for cage {id} leaked from another test"
        );

        add_cage(id, make_test_cage(id, id)); // self-parented: no zombie/child_num/signal path
        fdtables::init_empty_cage(id);
        let underfd = UNDERFD_BASE + 0x0FFF_0000;
        fdtables::get_unused_virtual_fd(id, TEST_FDKIND, underfd, false, 0).unwrap();

        let cage = get_cage(id).unwrap();
        cage.grate_inflight.store(1, Ordering::SeqCst);

        let entered = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (entered2, done2) = (Arc::clone(&entered), Arc::clone(&done));
        let handle = thread::spawn(move || {
            entered2.store(true, Ordering::Release);
            cage_finalize(id);
            done2.store(true, Ordering::Release);
        });

        // One-sided invariant: while grate_inflight != 0, finalize must not
        // have completed and the cage/fdtable must still be present. This is
        // recorded, not asserted directly, so the drain can always be
        // released below even if a violation is observed.
        let mut violation: Option<&'static str> = None;
        for k in 0..10_000u32 {
            if done.load(Ordering::Acquire) {
                violation = Some("cage_finalize completed while grate_inflight > 0");
                break;
            }
            if get_cage(id).is_none() {
                violation = Some("cage removed from CAGE_MAP while grate_inflight > 0");
                break;
            }
            if !fdtables::check_cage_exists(id) {
                violation = Some("fdtable removed while grate_inflight > 0");
                break;
            }
            if k % 64 == 0 {
                thread::yield_now();
            }
        }

        // Always release the drain before asserting: an assert-first here
        // could leave the finalize thread spinning inside cage_finalize
        // forever if a violation was observed above.
        cage.grate_inflight.store(0, Ordering::SeqCst);
        assert!(violation.is_none(), "{}", violation.unwrap_or_default());
        assert!(
            entered.load(Ordering::Acquire),
            "finalize thread never started"
        );

        let mut budget = 10_000_000u64;
        while !done.load(Ordering::Acquire) {
            assert!(
                budget > 0,
                "cage_finalize did not complete after grate_inflight reached 0"
            );
            budget -= 1;
            thread::yield_now();
        }
        handle.join().expect("finalize thread panicked");

        assert!(get_cage(id).is_none());
        assert!(!fdtables::check_cage_exists(id));
        assert_eq!(
            RELEASED.lock().iter().filter(|&&x| x == underfd).count(),
            1,
            "grate_inflight test's fd not released exactly once"
        );
        drop(cage);
    }
}
