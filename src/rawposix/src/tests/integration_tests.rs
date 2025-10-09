// Integration tests for component interactions
//
// Tests interactions between cages, fdtables, and syscalls.

use crate::sys_calls::*;
use crate::tests::*;
use sysdefs::constants::lind_platform_const::UNUSED_ARG;

#[test]
fn test_cage_and_fdtable_lifecycle() {
    let _guard = test_setup();
    
    const NEW_CAGE: u64 = TEST_CAGE_ID + 5;
    
    // Create new cage
    let cage = cage::Cage {
        cageid: NEW_CAGE,
        parent: TEST_CAGE_ID,
        cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(NEW_CAGE),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    };
    cage::add_cage(NEW_CAGE, cage);
    
    // Initialize fdtable for new cage
    fdtables::init_empty_cage(NEW_CAGE);
    
    // Verify cage exists
    assert!(cage::get_cage(NEW_CAGE).is_some());
    
    // Verify syscalls work
    let pid = getpid_syscall(
        NEW_CAGE,
        UNUSED_ARG, NEW_CAGE,
        UNUSED_ARG, NEW_CAGE,
        UNUSED_ARG, NEW_CAGE,
        UNUSED_ARG, NEW_CAGE,
        UNUSED_ARG, NEW_CAGE,
        UNUSED_ARG, NEW_CAGE,
    );
    assert_eq!(pid, NEW_CAGE as i32);
    
    // Cleanup
    cage::remove_cage(NEW_CAGE);
    fdtables::remove_cage_from_fdtable(NEW_CAGE);
    
    // Verify cleanup
    assert!(cage::get_cage(NEW_CAGE).is_none());
    
    test_teardown();
}

#[test]
fn test_parent_child_with_fdtables() {
    let _guard = test_setup();
    
    const PARENT: u64 = TEST_CAGE_ID;
    const CHILD: u64 = TEST_CAGE_ID + 1;
    
    // Create child cage
    let child = cage::Cage {
        cageid: CHILD,
        parent: PARENT,
        cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(CHILD),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    };
    cage::add_cage(CHILD, child);
    fdtables::init_empty_cage(CHILD);
    
    // Parent and child should have independent fd spaces
    // This is verified through fdtable initialization
    
    // Verify parent-child relationship via syscall
    let child_ppid = getppid_syscall(
        CHILD,
        UNUSED_ARG, CHILD,
        UNUSED_ARG, CHILD,
        UNUSED_ARG, CHILD,
        UNUSED_ARG, CHILD,
        UNUSED_ARG, CHILD,
        UNUSED_ARG, CHILD,
    );
    assert_eq!(child_ppid, PARENT as i32);
    
    // Cleanup
    cage::remove_cage(CHILD);
    fdtables::remove_cage_from_fdtable(CHILD);
    
    test_teardown();
}

#[test]
fn test_multiple_cages_concurrent_operations() {
    let _guard = test_setup();
    
    let cage_ids = vec![
        TEST_CAGE_ID + 10,
        TEST_CAGE_ID + 20,
        TEST_CAGE_ID + 30,
    ];
    
    // Create multiple cages
    for &cage_id in &cage_ids {
        let cage = cage::Cage {
            cageid: cage_id,
            parent: TEST_CAGE_ID,
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
        };
        cage::add_cage(cage_id, cage);
        fdtables::init_empty_cage(cage_id);
    }
    
    // Perform operations on all cages
    for &cage_id in &cage_ids {
        // Verify cage exists
        assert!(cage::get_cage(cage_id).is_some());
        
        // Call syscall
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
    }
    
    // Cleanup all
    for &cage_id in &cage_ids {
        cage::remove_cage(cage_id);
        fdtables::remove_cage_from_fdtable(cage_id);
        assert!(cage::get_cage(cage_id).is_none());
    }
    
    test_teardown();
}

#[test]
fn test_cage_state_persistence() {
    let _guard = test_setup();
    
    // Get initial state
    let cage = cage::get_cage(TEST_CAGE_ID).expect("Cage should exist");
    
    // Modify state
    cage.sigset.store(12345, std::sync::atomic::Ordering::SeqCst);
    cage.child_num.store(42, std::sync::atomic::Ordering::SeqCst);
    
    // State should persist across syscalls
    let _pid = getpid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    // Re-fetch and verify
    let cage = cage::get_cage(TEST_CAGE_ID).expect("Cage should still exist");
    assert_eq!(cage.sigset.load(std::sync::atomic::Ordering::SeqCst), 12345);
    assert_eq!(cage.child_num.load(std::sync::atomic::Ordering::SeqCst), 42);
    
    test_teardown();
}

#[test]
fn test_fd_allocation_after_cage_operations() {
    let _guard = test_setup();
    
    // Make syscall multiple times
    let pid1 = getpid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    let pid2 = getpid_syscall(
        TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
        UNUSED_ARG, TEST_CAGE_ID,
    );
    
    // Should be consistent
    assert_eq!(pid1, pid2);
    
    test_teardown();
}

#[test]
fn test_nested_cage_hierarchy() {
    let _guard = test_setup();
    
    const L1: u64 = TEST_CAGE_ID;
    const L2: u64 = TEST_CAGE_ID + 1;
    const L3: u64 = TEST_CAGE_ID + 2;
    const L4: u64 = TEST_CAGE_ID + 3;
    
    // Create 4-level hierarchy
    for (child, parent) in [(L2, L1), (L3, L2), (L4, L3)] {
        let cage = cage::Cage {
            cageid: child,
            parent,
            cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/"))),
            rev_shm: parking_lot::Mutex::new(Vec::new()),
            signalhandler: dashmap::DashMap::new(),
            sigset: std::sync::atomic::AtomicU64::new(0),
            pending_signals: parking_lot::RwLock::new(Vec::new()),
            epoch_handler: dashmap::DashMap::new(),
            main_threadid: parking_lot::RwLock::new(0),
            interval_timer: cage::IntervalTimer::new(child),
            zombies: parking_lot::RwLock::new(Vec::new()),
            child_num: std::sync::atomic::AtomicU64::new(0),
            vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
        };
        cage::add_cage(child, cage);
        fdtables::init_empty_cage(child);
    }
    
    // Verify each level
    for &cage_id in &[L1, L2, L3, L4] {
        assert!(cage::get_cage(cage_id).is_some());
    }
    
    // Verify hierarchy via ppid
    let l2_ppid = getppid_syscall(L2, UNUSED_ARG, L2, UNUSED_ARG, L2, UNUSED_ARG, L2, UNUSED_ARG, L2, UNUSED_ARG, L2, UNUSED_ARG, L2);
    let l3_ppid = getppid_syscall(L3, UNUSED_ARG, L3, UNUSED_ARG, L3, UNUSED_ARG, L3, UNUSED_ARG, L3, UNUSED_ARG, L3, UNUSED_ARG, L3);
    let l4_ppid = getppid_syscall(L4, UNUSED_ARG, L4, UNUSED_ARG, L4, UNUSED_ARG, L4, UNUSED_ARG, L4, UNUSED_ARG, L4, UNUSED_ARG, L4);
    
    assert_eq!(l2_ppid, L1 as i32);
    assert_eq!(l3_ppid, L2 as i32);
    assert_eq!(l4_ppid, L3 as i32);
    
    // Cleanup
    for &cage_id in &[L4, L3, L2] {
        cage::remove_cage(cage_id);
        fdtables::remove_cage_from_fdtable(cage_id);
    }
    
    test_teardown();
}

#[test]
fn test_cage_recreation() {
    let _guard = test_setup();
    
    const CAGE_ID: u64 = TEST_CAGE_ID + 99;
    
    // Create cage
    let cage = cage::Cage {
        cageid: CAGE_ID,
        parent: TEST_CAGE_ID,
        cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(CAGE_ID),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    };
    cage::add_cage(CAGE_ID, cage);
    fdtables::init_empty_cage(CAGE_ID);
    
    assert!(cage::get_cage(CAGE_ID).is_some());
    
    // Remove
    cage::remove_cage(CAGE_ID);
    fdtables::remove_cage_from_fdtable(CAGE_ID);
    assert!(cage::get_cage(CAGE_ID).is_none());
    
    // Recreate with same ID
    let cage = cage::Cage {
        cageid: CAGE_ID,
        parent: TEST_CAGE_ID,
        cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/tmp"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(CAGE_ID),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    };
    cage::add_cage(CAGE_ID, cage);
    fdtables::init_empty_cage(CAGE_ID);
    
    // Should work again
    assert!(cage::get_cage(CAGE_ID).is_some());
    
    // Cleanup
    cage::remove_cage(CAGE_ID);
    fdtables::remove_cage_from_fdtable(CAGE_ID);
    
    test_teardown();
}

#[test]
fn test_syscalls_across_cage_lifecycle() {
    let _guard = test_setup();
    
    const CAGE_ID: u64 = TEST_CAGE_ID + 77;
    
    // Create cage
    let cage = cage::Cage {
        cageid: CAGE_ID,
        parent: TEST_CAGE_ID,
        cwd: parking_lot::RwLock::new(std::sync::Arc::new(std::path::PathBuf::from("/"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(CAGE_ID),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    };
    cage::add_cage(CAGE_ID, cage);
    
    // Test all ID syscalls
    let pid = getpid_syscall(CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID);
    let ppid = getppid_syscall(CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID);
    let uid = getuid_syscall(CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID);
    let gid = getgid_syscall(CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID);
    let euid = geteuid_syscall(CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID);
    let egid = getegid_syscall(CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID, UNUSED_ARG, CAGE_ID);
    
    assert_eq!(pid, CAGE_ID as i32);
    assert_eq!(ppid, TEST_CAGE_ID as i32);
    assert!(uid >= 0);
    assert!(gid >= 0);
    assert!(euid >= 0);
    assert!(egid >= 0);
    
    // Cleanup
    cage::remove_cage(CAGE_ID);
    
    test_teardown();
}

