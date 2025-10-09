// System call tests for RawPOSIX
//
// Tests for process-related syscalls that work in Rust unit tests.
// These tests don't require WASM vmmap or complex signal/fork infrastructure.

use crate::sys_calls::*;
use crate::tests::*;
use sysdefs::constants::lind_platform_const::UNUSED_ARG;
use sysdefs::constants::sys_const::{DEFAULT_GID, DEFAULT_UID};

/// Test: getpid_syscall returns the cage ID as the process ID
/// Verifies that calling getpid returns the correct cage ID value
#[test]
fn test_getpid() {
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

/// Test: getuid_syscall returns the default user ID
/// Verifies that getuid returns DEFAULT_UID constant
#[test]
fn test_getuid() {
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
    assert_eq!(uid, DEFAULT_UID as i32);
    
    test_teardown();
}

/// Test: geteuid_syscall returns the effective user ID
/// Verifies that geteuid returns DEFAULT_UID (should match real UID)
#[test]
fn test_geteuid() {
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
    assert_eq!(euid, DEFAULT_UID as i32);
    
    test_teardown();
}

/// Test: getgid_syscall returns the default group ID
/// Verifies that getgid returns DEFAULT_GID constant
#[test]
fn test_getgid() {
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
    assert_eq!(gid, DEFAULT_GID as i32);
    
    test_teardown();
}

/// Test: getegid_syscall returns the effective group ID
/// Verifies that getegid returns DEFAULT_GID (should match real GID)
#[test]
fn test_getegid() {
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
    assert_eq!(egid, DEFAULT_GID as i32);
    
    test_teardown();
}

/// Test: getppid_syscall returns the parent process ID
/// Verifies that getppid returns TEST_CAGE_ID (test cage is its own parent)
#[test]
fn test_getppid() {
    let _guard = test_setup();
    
    // Since we set parent = cageid in test_setup, getppid should return TEST_CAGE_ID
    let ppid = getppid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    assert_eq!(ppid, TEST_CAGE_ID as i32);
    
    test_teardown();
}

// === ADDITIONAL VALIDATION TESTS ===

/// Test: getpid returns consistent value across multiple calls
/// Verifies that getpid returns the same value when called multiple times
#[test]
fn test_getpid_multiple_calls_consistent() {
    let _guard = test_setup();
    
    let pid1 = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let pid2 = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let pid3 = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    
    assert_eq!(pid1, pid2);
    assert_eq!(pid2, pid3);
    
    test_teardown();
}

/// Test: Real UID matches effective UID
/// Verifies that getuid and geteuid return the same value
#[test]
fn test_uid_matches_euid() {
    let _guard = test_setup();
    
    let uid = getuid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let euid = geteuid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    
    assert_eq!(uid, euid);
    
    test_teardown();
}

/// Test: Real GID matches effective GID
/// Verifies that getgid and getegid return the same value
#[test]
fn test_gid_matches_egid() {
    let _guard = test_setup();
    
    let gid = getgid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let egid = getegid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    
    assert_eq!(gid, egid);
    
    test_teardown();
}

/// Test: All ID syscalls return non-negative values
/// Verifies that pid, ppid, uid, gid are all >= 0
#[test]
fn test_all_ids_non_negative() {
    let _guard = test_setup();
    
    let pid = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let ppid = getppid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let uid = getuid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    let gid = getgid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
    
    assert!(pid >= 0);
    assert!(ppid >= 0);
    assert!(uid >= 0);
    assert!(gid >= 0);
    
    test_teardown();
}

/// Test: Rapid getpid calls maintain consistency (stress test)
/// Verifies that 50 consecutive getpid calls all return the same value
#[test]
fn test_rapid_getpid_calls() {
    let _guard = test_setup();
    
    for _ in 0..50 {
        let pid = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
        assert_eq!(pid, TEST_CAGE_ID as i32);
    }
    
    test_teardown();
}

/// Test: Interleaved syscalls maintain correctness
/// Verifies that mixing different syscalls returns correct values for each
#[test]
fn test_mixed_syscalls_sequence() {
    let _guard = test_setup();
    
    for i in 0..10 {
        match i % 6 {
            0 => {
                let pid = getpid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
                assert_eq!(pid, TEST_CAGE_ID as i32);
            },
            1 => {
                let ppid = getppid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
                assert_eq!(ppid, TEST_CAGE_ID as i32);
            },
            2 => {
                let uid = getuid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
                assert_eq!(uid, DEFAULT_UID as i32);
            },
            3 => {
                let euid = geteuid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
                assert_eq!(euid, DEFAULT_UID as i32);
            },
            4 => {
                let gid = getgid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
                assert_eq!(gid, DEFAULT_GID as i32);
            },
            _ => {
                let egid = getegid_syscall(TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID, UNUSED_ARG, TEST_CAGE_ID);
                assert_eq!(egid, DEFAULT_GID as i32);
            },
        }
    }
    
    test_teardown();
}
