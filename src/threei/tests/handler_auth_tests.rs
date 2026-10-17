//! Who may change a cage's routing: the checked `register_handler_syscall` and
//! `copy_handler_table_to_cage_syscall` that cages reach through syscalls 1001
//! and 1003.
use cage::timer::IntervalTimer;
use cage::{
    add_cage, cagetable_init, AtomicBool, AtomicU64, Cage, Condvar, Mutex, PathBuf, RwLock,
};
use serial_test::serial;
use std::sync::Arc;
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, THREEI_CAGEID, WASMTIME_CAGEID};
use threei::threei_const::{ELINDEPERM, THREEI_DEREGISTER};
use threei::{copy_handler_table_to_cage_syscall, make_syscall, register_handler_syscall};
mod common;
use common::*;

// Cage tree used by every test:
//
//   INIT (1)
//    └─ GRATE (10, exec'd)
//        ├─ CHILD (11, forked, not exec'd yet)
//        │   └─ LEAF (12, exec'd: an untrusted program such as cc1)
//        └─ SIBLING (13, exec'd)
const INIT: u64 = 1;
const GRATE: u64 = 10;
const CHILD: u64 = 11;
const LEAF: u64 = 12;
const SIBLING: u64 = 13;

const SYS_WRITE: u64 = 1;
const FN: u64 = 42;
const DENIED: i32 = ELINDEPERM as i32;

fn make_cage(cageid: u64, parent: u64, execd: bool) -> Cage {
    Cage {
        cageid,
        parent,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: Mutex::new(Vec::new()),
        signalhandler: Default::default(),
        sigset: AtomicU64::new(0),
        pending_signals: RwLock::new(vec![]),
        epoch_handler: Default::default(),
        os_tid_map: Default::default(),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(cageid),
        zombies: Mutex::new(vec![]),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(cage::Vmmap::new()),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
        execd: AtomicBool::new(execd),
    }
}

fn setup() {
    clear_globals();
    cagetable_init();
    add_cage(INIT, make_cage(INIT, INIT, true));
    add_cage(GRATE, make_cage(GRATE, INIT, true));
    add_cage(CHILD, make_cage(CHILD, GRATE, false));
    add_cage(LEAF, make_cage(LEAF, CHILD, true));
    add_cage(SIBLING, make_cage(SIBLING, GRATE, true));
}

/// `caller` asks to route `target`'s `callnum` to function `FN` of `handler`,
/// laid out as glibc's `register_handler` wrapper does.
fn reg(caller: u64, target: u64, callnum: u64, handler: u64) -> i32 {
    register_handler_syscall(
        handler, handler, target, callnum, 0, handler, FN, 0, 0, 0, 0, 0, 0, caller,
    )
}

/// `caller` asks to copy `src`'s handler table over `target`'s.
fn copy(caller: u64, src: u64, target: u64) -> i32 {
    copy_handler_table_to_cage_syscall(src, src, src, target, 0, 0, 0, 0, 0, 0, 0, 0, 0, caller)
}

#[test]
#[serial]
fn grate_routes_its_descendants_to_itself() {
    setup();
    assert_eq!(reg(GRATE, CHILD, SYS_WRITE, GRATE), 0);
    assert_eq!(reg(GRATE, LEAF, SYS_WRITE, GRATE), 0);
    assert_eq!(mappings_for(LEAF, SYS_WRITE), vec![(GRATE, FN)]);
}

#[test]
#[serial]
fn cage_cannot_reroute_an_ancestor_or_a_sibling() {
    setup();
    // The E probe pattern turned around: an untrusted leaf hijacking the grate.
    assert_eq!(reg(LEAF, GRATE, SYS_WRITE, LEAF), DENIED);
    assert_eq!(reg(LEAF, CHILD, SYS_WRITE, LEAF), DENIED);
    assert_eq!(reg(LEAF, SIBLING, SYS_WRITE, LEAF), DENIED);
    assert_eq!(reg(CHILD, SIBLING, SYS_WRITE, CHILD), DENIED);
    assert!(mappings_for(GRATE, SYS_WRITE).is_empty());
    assert!(mappings_for(SIBLING, SYS_WRITE).is_empty());
}

#[test]
#[serial]
fn pre_exec_child_may_route_its_own_calls_to_its_parent() {
    setup();
    assert_eq!(reg(CHILD, CHILD, SYS_WRITE, GRATE), 0);
    // Only its own calls, and only to its parent.
    assert_eq!(reg(CHILD, LEAF, SYS_WRITE, GRATE), DENIED);
    assert_eq!(reg(CHILD, CHILD, SYS_WRITE, INIT), DENIED);
}

#[test]
#[serial]
fn exec_d_cage_cannot_route_its_calls_into_an_ancestor() {
    setup();
    // LEAF runs its own program, so naming CHILD's or GRATE's functions as
    // handlers would let it call into code it does not control.
    assert_eq!(reg(LEAF, LEAF, SYS_WRITE, CHILD), DENIED);
    assert_eq!(reg(LEAF, LEAF, SYS_WRITE, GRATE), DENIED);
    assert_eq!(reg(LEAF, LEAF, SYS_WRITE, SIBLING), DENIED);
    // Its own functions are fine.
    assert_eq!(reg(LEAF, LEAF, SYS_WRITE, LEAF), 0);
}

#[test]
#[serial]
fn runtime_handler_cages_are_refused() {
    setup();
    for handler in [RAWPOSIX_CAGEID, WASMTIME_CAGEID, THREEI_CAGEID] {
        assert_eq!(reg(GRATE, CHILD, SYS_WRITE, handler), DENIED);
        assert_eq!(reg(LEAF, LEAF, SYS_WRITE, handler), DENIED);
    }
}

#[test]
#[serial]
fn deregister_needs_control_of_the_target() {
    setup();
    assert_eq!(reg(GRATE, LEAF, SYS_WRITE, GRATE), 0);
    assert_eq!(reg(SIBLING, LEAF, SYS_WRITE, THREEI_DEREGISTER), DENIED);
    assert_eq!(mappings_for(LEAF, SYS_WRITE), vec![(GRATE, FN)]);
    assert_eq!(reg(GRATE, LEAF, SYS_WRITE, THREEI_DEREGISTER), 0);
    assert!(mappings_for(LEAF, SYS_WRITE).is_empty());
}

#[test]
#[serial]
fn copy_needs_control_of_source_and_target() {
    setup();
    assert_eq!(reg(GRATE, CHILD, SYS_WRITE, GRATE), 0);
    // A grate propagating a descendant's routing on fork.
    assert_eq!(copy(GRATE, CHILD, LEAF), 0);
    assert_eq!(mappings_for(LEAF, SYS_WRITE), vec![(GRATE, FN)]);
    // A cage taking the routing of a cage above it steps around that grate.
    assert_eq!(copy(LEAF, GRATE, LEAF), DENIED);
    assert_eq!(copy(CHILD, INIT, CHILD), DENIED);
    // Nor may it overwrite a cage it does not control.
    assert_eq!(copy(LEAF, LEAF, SIBLING), DENIED);
    assert_eq!(mappings_for(LEAF, SYS_WRITE), vec![(GRATE, FN)]);
}

#[test]
#[serial]
fn syscall_with_no_handler_returns_enosys_not_panic() {
    setup();
    // A cage (self == target) whose table holds no handler for this syscall
    // must get an error for that one call, not bring down the enclave. Before
    // the self_cageid spoof fix a bogus self_cageid reached this path and
    // panicked.
    let rc = make_syscall(LEAF, SYS_WRITE, 0, LEAF, 0, LEAF, 0, LEAF, 0, LEAF, 0, LEAF, 0, LEAF, 0, LEAF);
    assert_eq!(rc, -(Errno::ENOSYS as i32));
}
