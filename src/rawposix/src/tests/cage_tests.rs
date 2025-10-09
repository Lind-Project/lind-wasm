// Cage component unit tests
//
// Comprehensive tests for cage lifecycle, state management, and isolation.

use crate::tests::*;
use cage::{add_cage, get_cage, remove_cage, Cage};

// === BASIC CAGE OPERATIONS ===

/// Test: Cage exists after test_setup completes
/// Verifies that test_setup successfully creates a cage
#[test]
fn test_cage_exists_after_setup() {
    let _guard = test_setup();
    assert!(get_cage(TEST_CAGE_ID).is_some());
    test_teardown();
}

/// Test: Cage has the correct ID assigned
/// Verifies that the cage's cageid field matches TEST_CAGE_ID
#[test]
fn test_cage_has_correct_id() {
    let _guard = test_setup();
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    assert_eq!(cage.cageid, TEST_CAGE_ID);
    test_teardown();
}

/// Test: Root cage's parent is itself
/// Verifies that test cage is set as its own parent (root cage behavior)
#[test]
fn test_cage_parent_is_self() {
    let _guard = test_setup();
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    assert_eq!(cage.parent, TEST_CAGE_ID);
    test_teardown();
}

/// Test: Cage starts with default working directory
/// Verifies that initial cwd is "/" (root directory)
#[test]
fn test_cage_default_cwd() {
    let _guard = test_setup();
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    let cwd = cage.cwd.read();
    assert_eq!(cwd.to_str().unwrap(), "/");
    test_teardown();
}

/// Test: Cage signal set is initialized to zero
/// Verifies that sigset starts with no signals blocked
#[test]
fn test_cage_initial_sigset() {
    let _guard = test_setup();
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    let sigset = cage.sigset.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(sigset, 0);
    test_teardown();
}

/// Test: Cage child counter starts at zero
/// Verifies that child_num is initialized to 0 for new cage
#[test]
fn test_cage_initial_child_count() {
    let _guard = test_setup();
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    let child_num = cage.child_num.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(child_num, 0);
    test_teardown();
}

// === CAGE LIFECYCLE ===

/// Test: Creating and removing a cage works correctly
/// Verifies that add_cage creates a cage and remove_cage deletes it
#[test]
fn test_create_and_remove_cage() {
    let _guard = test_setup();
    
    const NEW_CAGE: u64 = TEST_CAGE_ID + 1;
    let cage = create_test_cage(NEW_CAGE, TEST_CAGE_ID);
    add_cage(NEW_CAGE, cage);
    
    assert!(get_cage(NEW_CAGE).is_some());
    remove_cage(NEW_CAGE);
    assert!(get_cage(NEW_CAGE).is_none());
    
    test_teardown();
}

/// Test: Removing non-existent cage doesn't panic
/// Verifies that remove_cage is safe to call on non-existent cage ID
#[test]
fn test_remove_nonexistent_cage() {
    let _guard = test_setup();
    
    const NONEXISTENT: u64 = TEST_CAGE_ID + 999;
    remove_cage(NONEXISTENT); // Should not panic
    
    test_teardown();
}

/// Test: Multiple cages can be created and managed independently
/// Verifies that 3 cages can be created, exist simultaneously, and be removed
#[test]
fn test_multiple_cage_creation() {
    let _guard = test_setup();
    
    let cage_ids = vec![
        TEST_CAGE_ID + 1,
        TEST_CAGE_ID + 2,
        TEST_CAGE_ID + 3,
    ];
    
    for &cage_id in &cage_ids {
        let cage = create_test_cage(cage_id, TEST_CAGE_ID);
        add_cage(cage_id, cage);
        assert!(get_cage(cage_id).is_some());
    }
    
    for &cage_id in &cage_ids {
        remove_cage(cage_id);
        assert!(get_cage(cage_id).is_none());
    }
    
    test_teardown();
}

// === STATE MANAGEMENT ===

#[test]
fn test_modify_sigset() {
    let _guard = test_setup();
    
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    cage.sigset.store(42, std::sync::atomic::Ordering::SeqCst);
    
    let value = cage.sigset.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(value, 42);
    
    test_teardown();
}

#[test]
fn test_increment_child_count() {
    let _guard = test_setup();
    
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    cage.child_num.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    cage.child_num.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    
    let count = cage.child_num.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 2);
    
    test_teardown();
}

#[test]
fn test_modify_cwd() {
    let _guard = test_setup();
    
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    let mut cwd = cage.cwd.write();
    *cwd = std::sync::Arc::new(std::path::PathBuf::from("/tmp"));
    drop(cwd);
    
    let cwd = cage.cwd.read();
    assert_eq!(cwd.to_str().unwrap(), "/tmp");
    
    test_teardown();
}

#[test]
fn test_cwd_persistence() {
    let _guard = test_setup();
    
    let cage = get_cage(TEST_CAGE_ID).expect("Cage should exist");
    
    {
        let mut cwd = cage.cwd.write();
        *cwd = std::sync::Arc::new(std::path::PathBuf::from("/home"));
    }
    
    // Verify it persists
    let cwd = cage.cwd.read();
    assert_eq!(cwd.to_str().unwrap(), "/home");
    
    test_teardown();
}

// === PARENT-CHILD RELATIONSHIPS ===

#[test]
fn test_parent_child_relationship() {
    let _guard = test_setup();
    
    const CHILD: u64 = TEST_CAGE_ID + 1;
    let cage = create_test_cage(CHILD, TEST_CAGE_ID);
    add_cage(CHILD, cage);
    
    let child_cage = get_cage(CHILD).expect("Child should exist");
    assert_eq!(child_cage.parent, TEST_CAGE_ID);
    
    remove_cage(CHILD);
    test_teardown();
}

#[test]
fn test_multi_level_hierarchy() {
    let _guard = test_setup();
    
    const L1: u64 = TEST_CAGE_ID;
    const L2: u64 = TEST_CAGE_ID + 1;
    const L3: u64 = TEST_CAGE_ID + 2;
    
    let cage_l2 = create_test_cage(L2, L1);
    add_cage(L2, cage_l2);
    
    let cage_l3 = create_test_cage(L3, L2);
    add_cage(L3, cage_l3);
    
    let cage2 = get_cage(L2).unwrap();
    let cage3 = get_cage(L3).unwrap();
    
    assert_eq!(cage2.parent, L1);
    assert_eq!(cage3.parent, L2);
    
    remove_cage(L3);
    remove_cage(L2);
    test_teardown();
}

#[test]
fn test_siblings_share_parent() {
    let _guard = test_setup();
    
    const CHILD1: u64 = TEST_CAGE_ID + 1;
    const CHILD2: u64 = TEST_CAGE_ID + 2;
    
    let cage1 = create_test_cage(CHILD1, TEST_CAGE_ID);
    add_cage(CHILD1, cage1);
    
    let cage2 = create_test_cage(CHILD2, TEST_CAGE_ID);
    add_cage(CHILD2, cage2);
    
    let c1 = get_cage(CHILD1).unwrap();
    let c2 = get_cage(CHILD2).unwrap();
    
    assert_eq!(c1.parent, TEST_CAGE_ID);
    assert_eq!(c2.parent, TEST_CAGE_ID);
    assert_eq!(c1.parent, c2.parent);
    
    remove_cage(CHILD1);
    remove_cage(CHILD2);
    test_teardown();
}

// === ISOLATION TESTS ===

#[test]
fn test_cages_have_independent_cwd() {
    let _guard = test_setup();
    
    const CAGE_A: u64 = TEST_CAGE_ID + 1;
    const CAGE_B: u64 = TEST_CAGE_ID + 2;
    
    let cage_a = create_test_cage(CAGE_A, TEST_CAGE_ID);
    add_cage(CAGE_A, cage_a);
    
    let cage_b = create_test_cage(CAGE_B, TEST_CAGE_ID);
    add_cage(CAGE_B, cage_b);
    
    // Set different cwds
    let a = get_cage(CAGE_A).unwrap();
    let mut cwd_a = a.cwd.write();
    *cwd_a = std::sync::Arc::new(std::path::PathBuf::from("/tmp"));
    drop(cwd_a);
    
    let b = get_cage(CAGE_B).unwrap();
    let mut cwd_b = b.cwd.write();
    *cwd_b = std::sync::Arc::new(std::path::PathBuf::from("/home"));
    drop(cwd_b);
    
    // Verify independence
    let cwd_a = a.cwd.read();
    let cwd_b = b.cwd.read();
    
    assert_eq!(cwd_a.to_str().unwrap(), "/tmp");
    assert_eq!(cwd_b.to_str().unwrap(), "/home");
    assert_ne!(cwd_a.to_str(), cwd_b.to_str());
    
    remove_cage(CAGE_A);
    remove_cage(CAGE_B);
    test_teardown();
}

#[test]
fn test_cages_have_independent_sigset() {
    let _guard = test_setup();
    
    const CAGE_A: u64 = TEST_CAGE_ID + 1;
    const CAGE_B: u64 = TEST_CAGE_ID + 2;
    
    let cage_a = create_test_cage(CAGE_A, TEST_CAGE_ID);
    add_cage(CAGE_A, cage_a);
    
    let cage_b = create_test_cage(CAGE_B, TEST_CAGE_ID);
    add_cage(CAGE_B, cage_b);
    
    let a = get_cage(CAGE_A).unwrap();
    let b = get_cage(CAGE_B).unwrap();
    
    a.sigset.store(100, std::sync::atomic::Ordering::SeqCst);
    b.sigset.store(200, std::sync::atomic::Ordering::SeqCst);
    
    assert_eq!(a.sigset.load(std::sync::atomic::Ordering::SeqCst), 100);
    assert_eq!(b.sigset.load(std::sync::atomic::Ordering::SeqCst), 200);
    
    remove_cage(CAGE_A);
    remove_cage(CAGE_B);
    test_teardown();
}

#[test]
fn test_removing_one_cage_doesnt_affect_others() {
    let _guard = test_setup();
    
    const CAGE_A: u64 = TEST_CAGE_ID + 1;
    const CAGE_B: u64 = TEST_CAGE_ID + 2;
    
    let cage_a = create_test_cage(CAGE_A, TEST_CAGE_ID);
    add_cage(CAGE_A, cage_a);
    
    let cage_b = create_test_cage(CAGE_B, TEST_CAGE_ID);
    add_cage(CAGE_B, cage_b);
    
    assert!(get_cage(CAGE_A).is_some());
    assert!(get_cage(CAGE_B).is_some());
    
    remove_cage(CAGE_A);
    
    assert!(get_cage(CAGE_A).is_none());
    assert!(get_cage(CAGE_B).is_some());
    
    remove_cage(CAGE_B);
    test_teardown();
}

// === HELPER FUNCTION ===

fn create_test_cage(cage_id: u64, parent_id: u64) -> Cage {
    Cage {
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
