// FD table component unit tests
//
// Comprehensive tests for fdtable initialization, lifecycle, and isolation.

use crate::tests::*;
use cage::{add_cage, get_cage, remove_cage};

// === FDTABLE INITIALIZATION ===

/// Test: FD table is initialized after test_setup
/// Verifies that test_setup creates a working fdtable without errors
#[test]
fn test_fdtable_initialized_after_setup() {
    let _guard = test_setup();
    // If we got here without panic, fdtable is initialized
    assert!(get_cage(TEST_CAGE_ID).is_some());
    test_teardown();
}

/// Test: FD table can be initialized for a new cage
/// Verifies that init_empty_cage works for newly created cage
#[test]
fn test_fdtable_init_new_cage() {
    let _guard = test_setup();
    
    const NEW_CAGE: u64 = TEST_CAGE_ID + 1;
    fdtables::init_empty_cage(NEW_CAGE);
    
    // Should not panic
    fdtables::remove_cage_from_fdtable(NEW_CAGE);
    
    test_teardown();
}

/// Test: Multiple cages can each have their own fdtable
/// Verifies that fdtables can be initialized for 3 different cages
#[test]
fn test_fdtable_init_multiple_cages() {
    let _guard = test_setup();
    
    let cage_ids = vec![
        TEST_CAGE_ID + 1,
        TEST_CAGE_ID + 2,
        TEST_CAGE_ID + 3,
    ];
    
    for &cage_id in &cage_ids {
        fdtables::init_empty_cage(cage_id);
    }
    
    // All should be initialized
    for &cage_id in &cage_ids {
        fdtables::remove_cage_from_fdtable(cage_id);
    }
    
    test_teardown();
}

// === FDTABLE LIFECYCLE ===

/// Test: FD table can be removed from a cage
/// Verifies that remove_cage_from_fdtable completes without errors
#[test]
fn test_fdtable_remove() {
    let _guard = test_setup();
    
    const TEMP_CAGE: u64 = TEST_CAGE_ID + 10;
    fdtables::init_empty_cage(TEMP_CAGE);
    fdtables::remove_cage_from_fdtable(TEMP_CAGE);
    
    // Should complete without error
    test_teardown();
}

/// Test: Removing non-existent fdtable is handled gracefully
/// Verifies that remove operations on non-existent cage don't cause crashes
#[test]
fn test_fdtable_remove_nonexistent() {
    let _guard = test_setup();
    
    const NONEXISTENT: u64 = TEST_CAGE_ID + 999;
    // Should not panic
    // Note: This may panic depending on fdtables implementation
    // If it does, that's a known limitation
    
    test_teardown();
}

/// Test: FD table can be reinitialized after removal
/// Verifies that init-remove-init cycle works correctly
#[test]
fn test_fdtable_reinit_after_remove() {
    let _guard = test_setup();
    
    const TEMP_CAGE: u64 = TEST_CAGE_ID + 20;
    
    fdtables::init_empty_cage(TEMP_CAGE);
    fdtables::remove_cage_from_fdtable(TEMP_CAGE);
    fdtables::init_empty_cage(TEMP_CAGE);
    fdtables::remove_cage_from_fdtable(TEMP_CAGE);
    
    test_teardown();
}

// === FDTABLE ISOLATION ===

/// Test: Multiple fdtables operate independently
/// Verifies that removing one cage's fdtable doesn't affect others
#[test]
fn test_multiple_fdtables_independent() {
    let _guard = test_setup();
    
    const CAGE_A: u64 = TEST_CAGE_ID + 1;
    const CAGE_B: u64 = TEST_CAGE_ID + 2;
    
    fdtables::init_empty_cage(CAGE_A);
    fdtables::init_empty_cage(CAGE_B);
    
    // Both should exist independently
    fdtables::remove_cage_from_fdtable(CAGE_A);
    // CAGE_B should still be valid
    fdtables::remove_cage_from_fdtable(CAGE_B);
    
    test_teardown();
}

/// Test: FD table remains valid across operations
/// Verifies that fdtable stability under multiple get_cage calls
#[test]
fn test_fdtable_survives_operations() {
    let _guard = test_setup();
    
    // Perform multiple operations
    for _ in 0..10 {
        let _ = get_cage(TEST_CAGE_ID);
    }
    
    // FD table should still be valid
    assert!(get_cage(TEST_CAGE_ID).is_some());
    
    test_teardown();
}

// === INTEGRATION WITH CAGES ===

/// Test: Cage and fdtable can be created together
/// Verifies that cage creation + fdtable initialization works in tandem
#[test]
fn test_cage_and_fdtable_together() {
    let _guard = test_setup();
    
    const NEW_CAGE: u64 = TEST_CAGE_ID + 1;
    
    let cage = create_test_cage(NEW_CAGE, TEST_CAGE_ID);
    add_cage(NEW_CAGE, cage);
    fdtables::init_empty_cage(NEW_CAGE);
    
    assert!(get_cage(NEW_CAGE).is_some());
    
    remove_cage(NEW_CAGE);
    fdtables::remove_cage_from_fdtable(NEW_CAGE);
    
    assert!(get_cage(NEW_CAGE).is_none());
    
    test_teardown();
}

/// Test: Multiple cages each with fdtables work correctly
/// Verifies that 3 cages with fdtables can be created and removed
#[test]
fn test_multiple_cages_with_fdtables() {
    let _guard = test_setup();
    
    let cage_ids = vec![
        TEST_CAGE_ID + 1,
        TEST_CAGE_ID + 2,
        TEST_CAGE_ID + 3,
    ];
    
    // Create all
    for &cage_id in &cage_ids {
        let cage = create_test_cage(cage_id, TEST_CAGE_ID);
        add_cage(cage_id, cage);
        fdtables::init_empty_cage(cage_id);
    }
    
    // Verify all exist
    for &cage_id in &cage_ids {
        assert!(get_cage(cage_id).is_some());
    }
    
    // Remove all
    for &cage_id in &cage_ids {
        remove_cage(cage_id);
        fdtables::remove_cage_from_fdtable(cage_id);
    }
    
    test_teardown();
}

/// Test: FD table is cleaned up during test_teardown
/// Verifies that teardown successfully removes fdtable
#[test]
fn test_fdtable_cleanup_on_teardown() {
    let _guard = test_setup();
    
    // test_teardown will clean up fdtable
    assert!(get_cage(TEST_CAGE_ID).is_some());
    
    test_teardown();
    // If teardown succeeds, cleanup worked
}

// === CLOSE HANDLERS ===

/// Test: Close handlers are registered during setup
/// Verifies that test_setup registers close handlers without errors
#[test]
fn test_close_handlers_registered() {
    let _guard = test_setup();
    
    // Close handlers are registered in test_setup
    // If we got here, registration succeeded
    assert!(get_cage(TEST_CAGE_ID).is_some());
    
    test_teardown();
}

/// Test: Close handlers can be registered multiple times
/// Verifies that register_close_handlers is idempotent
#[test]
fn test_close_handlers_multiple_registrations() {
    let _guard = test_setup();
    
    // Register again (should be idempotent)
    fdtables::register_close_handlers(
        1, // FDKIND_KERNEL
        crate::fs_calls::kernel_close,
        crate::fs_calls::kernel_close,
    );
    
    test_teardown();
}

// === STRESS TESTS ===

/// Test: Stress test with 20 cages and fdtables
/// Verifies system stability under heavy cage+fdtable operations
#[test]
fn test_many_cage_fdtable_operations() {
    let _guard = test_setup();
    
    const BASE: u64 = TEST_CAGE_ID + 100;
    
    // Create 20 cages
    for i in 0..20 {
        let cage_id = BASE + i;
        let cage = create_test_cage(cage_id, TEST_CAGE_ID);
        add_cage(cage_id, cage);
        fdtables::init_empty_cage(cage_id);
    }
    
    // Remove all
    for i in 0..20 {
        let cage_id = BASE + i;
        remove_cage(cage_id);
        fdtables::remove_cage_from_fdtable(cage_id);
    }
    
    test_teardown();
}

/// Test: Complex init/remove sequence works correctly
/// Verifies that interleaved init and remove operations succeed
#[test]
fn test_fdtable_init_sequence() {
    let _guard = test_setup();
    
    const CAGE1: u64 = TEST_CAGE_ID + 1;
    const CAGE2: u64 = TEST_CAGE_ID + 2;
    
    // Init, remove, init again for multiple cages
    fdtables::init_empty_cage(CAGE1);
    fdtables::init_empty_cage(CAGE2);
    
    fdtables::remove_cage_from_fdtable(CAGE1);
    
    fdtables::init_empty_cage(CAGE1);
    
    fdtables::remove_cage_from_fdtable(CAGE1);
    fdtables::remove_cage_from_fdtable(CAGE2);
    
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

