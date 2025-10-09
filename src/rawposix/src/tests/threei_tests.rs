// 3i (Three Eye) integration tests
//
// Tests for 3i type conversion, cage ID argument passing, and syscall routing.

use crate::sys_calls::*;
use crate::tests::*;
use sysdefs::constants::lind_platform_const::UNUSED_ARG;
use sysdefs::constants::sys_const::{DEFAULT_GID, DEFAULT_UID};

// === CAGE ID PASSING ===

/// Test: Syscall routes to correct cage using cage ID
/// Verifies that cage ID argument correctly identifies the target cage
#[test]
fn test_syscall_with_correct_cage_id() {
    let _guard = test_setup();
    
    let pid = getpid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert_eq!(pid, TEST_CAGE_ID as i32);
    
    test_teardown();
}

/// Test: Different cages return different PIDs
/// Verifies that each cage ID maps to a distinct process ID
#[test]
fn test_multiple_cages_distinct_pids() {
    let _guard = test_setup();
    
    const CAGE_A: u64 = TEST_CAGE_ID + 1;
    const CAGE_B: u64 = TEST_CAGE_ID + 2;
    
    let cage_a = create_test_cage(CAGE_A, TEST_CAGE_ID);
    cage::add_cage(CAGE_A, cage_a);
    
    let cage_b = create_test_cage(CAGE_B, TEST_CAGE_ID);
    cage::add_cage(CAGE_B, cage_b);
    
    let pid_a = getpid_syscall(
        CAGE_A,
        UNUSED_ARG, CAGE_A,
        UNUSED_ARG, CAGE_A,
        UNUSED_ARG, CAGE_A,
        UNUSED_ARG, CAGE_A,
        UNUSED_ARG, CAGE_A,
        UNUSED_ARG, CAGE_A,
    );
    
    let pid_b = getpid_syscall(
        CAGE_B,
        UNUSED_ARG, CAGE_B,
        UNUSED_ARG, CAGE_B,
        UNUSED_ARG, CAGE_B,
        UNUSED_ARG, CAGE_B,
        UNUSED_ARG, CAGE_B,
        UNUSED_ARG, CAGE_B,
    );
    
    assert_eq!(pid_a, CAGE_A as i32);
    assert_eq!(pid_b, CAGE_B as i32);
    assert_ne!(pid_a, pid_b);
    
    cage::remove_cage(CAGE_A);
    cage::remove_cage(CAGE_B);
    
    test_teardown();
}

// === ARGUMENT CONVERSION ===

/// Test: UNUSED_ARG constants are handled correctly
/// Verifies that syscalls with UNUSED_ARG parameters work properly
#[test]
fn test_unused_arg_conversion() {
    let _guard = test_setup();
    
    // All UNUSED_ARG should be handled correctly
    let uid = getuid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert_eq!(uid, DEFAULT_UID as i32);
    
    test_teardown();
}

/// Test: Syscalls return consistent values across calls
/// Verifies that repeated calls to same syscall return same value
#[test]
fn test_consistent_return_values() {
    let _guard = test_setup();
    
    // Same syscall should return same value
    let uid1 = getuid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    let uid2 = getuid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert_eq!(uid1, uid2);
    
    test_teardown();
}

// === SYSCALL ROUTING ===

/// Test: getpid routes correctly through 3i layer
/// Verifies that getpid syscall routing returns non-negative PID
#[test]
fn test_getpid_routing() {
    let _guard = test_setup();
    
    let pid = getpid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert!(pid >= 0);
    
    test_teardown();
}

/// Test: getppid routes correctly through 3i layer
/// Verifies that getppid syscall routing returns non-negative PPID
#[test]
fn test_getppid_routing() {
    let _guard = test_setup();
    
    let ppid = getppid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert!(ppid >= 0);
    
    test_teardown();
}

/// Test: getuid routes correctly through 3i layer
/// Verifies that getuid syscall routing returns non-negative UID
#[test]
fn test_getuid_routing() {
    let _guard = test_setup();
    
    let uid = getuid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert!(uid >= 0);
    
    test_teardown();
}

/// Test: geteuid routes correctly through 3i layer
/// Verifies that geteuid syscall routing returns non-negative effective UID
#[test]
fn test_geteuid_routing() {
    let _guard = test_setup();
    
    let euid = geteuid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert!(euid >= 0);
    
    test_teardown();
}

/// Test: getgid routes correctly through 3i layer
/// Verifies that getgid syscall routing returns non-negative GID
#[test]
fn test_getgid_routing() {
    let _guard = test_setup();
    
    let gid = getgid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert!(gid >= 0);
    
    test_teardown();
}

/// Test: getegid routes correctly through 3i layer
/// Verifies that getegid syscall routing returns non-negative effective GID
#[test]
fn test_getegid_routing() {
    let _guard = test_setup();
    
    let egid = getegid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert!(egid >= 0);
    
    test_teardown();
}

// === TYPE SAFETY ===

/// Test: Syscalls return i32 type as expected
/// Verifies type safety of syscall return values
#[test]
fn test_return_values_are_i32() {
    let _guard = test_setup();
    
    let pid: i32 = getpid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    assert_eq!(pid, TEST_CAGE_ID as i32);
    
    test_teardown();
}

/// Test: Cage IDs are u64 type as expected
/// Verifies type safety of cage ID parameters
#[test]
fn test_cage_id_is_u64() {
    let _guard = test_setup();
    
    let cage_id: u64 = TEST_CAGE_ID;
    
    let pid = getpid_syscall(
        cage_id,
        UNUSED_ARG, cage_id,
        UNUSED_ARG, cage_id,
        UNUSED_ARG, cage_id,
        UNUSED_ARG, cage_id,
        UNUSED_ARG, cage_id,
        UNUSED_ARG, cage_id,
    );
    
    assert_eq!(pid, cage_id as i32);
    
    test_teardown();
}

// === CONCURRENT CALLS ===

/// Test: 100 rapid syscalls maintain correctness (stress test)
/// Verifies syscall stability under high-frequency calls
#[test]
fn test_rapid_syscalls() {
    let _guard = test_setup();
    
    for _ in 0..100 {
        let pid = getpid_syscall(
            TEST_CAGE_ID,
            UNUSED_ARG, TEST_CAGE_ID,
            UNUSED_ARG, TEST_CAGE_ID,
            UNUSED_ARG, TEST_CAGE_ID,
            UNUSED_ARG, TEST_CAGE_ID,
            UNUSED_ARG, TEST_CAGE_ID,
            UNUSED_ARG, TEST_CAGE_ID,
        );
        
        assert_eq!(pid, TEST_CAGE_ID as i32);
    }
    
    test_teardown();
}

/// Test: Interleaved different syscalls maintain correctness
/// Verifies that mixing multiple syscall types works correctly
#[test]
fn test_interleaved_syscalls() {
    let _guard = test_setup();
    
    for _ in 0..10 {
        let pid = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
        let uid = getuid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
        let gid = getgid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
        
        assert_eq!(pid, TEST_CAGE_ID as i32);
        assert_eq!(uid, DEFAULT_UID as i32);
        assert_eq!(gid, DEFAULT_GID as i32);
    }
    
    test_teardown();
}

// === HELPER FUNCTION ===

fn create_test_cage(cage_id: u64, parent_id: u64) -> cage::Cage {
    cage::Cage {
        cageid: cage_id,
        parent: parent_id,
        cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(cage_id),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    }
}

