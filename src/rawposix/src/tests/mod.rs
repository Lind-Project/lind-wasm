// RawPOSIX Test Suite
//
// Enterprise-grade unit tests for POSIX syscall implementations using the threei system.
//
// These tests verify core functionality without requiring WASM vmmap translation:
// - Cage lifecycle and state management
// - FD table operations and isolation
// - Syscall argument validation
// - Component integration
//
// For comprehensive syscall testing with memory translation, use C integration tests
// in tests/unit-tests/

mod sys_tests;          // System call tests (getpid, getuid, etc.)
mod cage_tests;         // Cage lifecycle and state management tests
mod fdtable_tests;      // FD table operations and isolation tests
mod threei_tests;       // 3i type conversion and routing tests

use cage::{add_cage, cagetable_init, get_cage, remove_cage, Cage};
use fdtables;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

// Global test mutex to prevent concurrent test execution
// Tests modify global state (CAGE_MAP, FDTABLE, etc.) so must run serially
static TEST_MUTEX: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(true));

/// Setup function for tests
/// Returns a lock guard that keeps the test serialized
pub fn test_setup() -> parking_lot::MutexGuard<'static, bool> {
    let guard = TEST_MUTEX.lock();
    
    // Initialize cage table if not already done
    unsafe {
        if cage::CAGE_MAP.is_empty() {
            cagetable_init();
        }
    }
    
    // Initialize fdtables for test cage
    fdtables::init_empty_cage(TEST_CAGE_ID);
    
    // Clean up any existing test cage
    remove_cage(TEST_CAGE_ID);
    
    // Create fresh test cage
    // Set parent = cageid to make this a "root" cage that won't send signals
    let test_cage = Cage {
        cageid: TEST_CAGE_ID,
        parent: TEST_CAGE_ID, // Self as parent to avoid SIGCHLD sending
        cwd: parking_lot::RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: parking_lot::Mutex::new(Vec::new()),
        signalhandler: dashmap::DashMap::new(),
        sigset: std::sync::atomic::AtomicU64::new(0),
        pending_signals: parking_lot::RwLock::new(Vec::new()),
        epoch_handler: dashmap::DashMap::new(),
        main_threadid: parking_lot::RwLock::new(0),
        interval_timer: cage::IntervalTimer::new(TEST_CAGE_ID),
        zombies: parking_lot::RwLock::new(Vec::new()),
        child_num: std::sync::atomic::AtomicU64::new(0),
        vmmap: parking_lot::RwLock::new(cage::Vmmap::new()),
    };
    
    add_cage(TEST_CAGE_ID, test_cage);
    
    // Register close handlers for fdtables (fdkind, intermediate, last)
    // Using FDKIND_KERNEL (1) for kernel file descriptors
    fdtables::register_close_handlers(
        1, // FDKIND_KERNEL
        crate::fs_calls::kernel_close,
        crate::fs_calls::kernel_close,
    );
    
    guard
}

/// Teardown function for tests
pub fn test_teardown() {
    // Remove test cage (if it still exists)
    if get_cage(TEST_CAGE_ID).is_some() {
        remove_cage(TEST_CAGE_ID);
        // Clean up fdtables
        fdtables::remove_cage_from_fdtable(TEST_CAGE_ID);
    }
}

/// Test cage ID used for all tests
pub const TEST_CAGE_ID: u64 = 999;

/// Helper to convert Rust string to C-style buffer pointer
pub fn str2cbuf(s: &str) -> *const u8 {
    s.as_ptr()
}

/// Helper to allocate a buffer of given size
pub fn alloc_buffer(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

/// Helper to convert buffer to string
pub fn buf2str(buf: &[u8]) -> &str {
    std::str::from_utf8(buf).unwrap()
}

