use cage::*;
use fdtables;
use cage::memory::vmmap::*;
use crate::fs_calls::{chdir_syscall, mkdir_syscall, open_syscall};

use std::ffi::CString;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, instrument};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::fs;
use libc::{pipe, read, write, close, c_void};

const FDKIND_KERNEL: u32 = 0;

/// Helper functions:
/// Create a test cage for testing purpose
fn simple_init_cage(cageid: u64) {
    println!("simple_init_cage called with cageid: {}", cageid);
    // fdtables::register_close_handlers(FDKIND_KERNEL, fdtables::NULL_FUNC, kernel_close);
    let cage = Cage {
        cageid: cageid,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        parent: 1,
        gid: AtomicI32::new(-1),
        uid: AtomicI32::new(-1),
        egid: AtomicI32::new(-1),
        euid: AtomicI32::new(-1),
        main_threadid: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(Vmmap::new()),
    };
    add_cage(cage);
    fdtables::init_empty_cage(cageid);
    println!("ADDED cage {:?} to fdtable", cageid);
    // Set the first 3 fd to STDIN / STDOUT / STDERR
    // STDIN
    // let dev_null = CString::new("/home/lind/lind_project/src/safeposix-rust/tmp/dev/null").unwrap();
    fdtables::get_specific_virtual_fd(cageid, 0, FDKIND_KERNEL, 0, false, 0).unwrap();
    // STDOUT
    fdtables::get_specific_virtual_fd(cageid, 1, FDKIND_KERNEL, 1, false, 0).unwrap();
    // STDERR
    fdtables::get_specific_virtual_fd(cageid, 2, FDKIND_KERNEL, 2, false, 0).unwrap();
}

/// Test read syscall behavior
#[test]
fn test_read() {
    let cageid = 52;
    simple_init_cage(cageid);

    let mut pipe_fds = [-1; 2];
    pipe(cageid, pipe_fds.as_mut_ptr());

    // Test reading with different buffer sizes
    let test_data = b"Testing read syscall";
    write(
        cageid,
        pipe_fds[1],
        test_data.as_ptr() as *const c_void,
        test_data.len(),
    );

    // Test partial read
    let mut small_buffer = vec![0u8; 5];
    let read_result = read(
        cageid,
        pipe_fds[0],
        small_buffer.as_mut_ptr() as *mut c_void,
        small_buffer.len(),
    );
    assert_eq!(read_result as usize, 5, "partial read failed");
    assert_eq!(&small_buffer, &test_data[..5], "partial read data mismatch");

    testing_remove_all();
}

/// Test close syscall behavior
#[test]
fn test_close() {
    let cageid = 53;
    simple_init_cage(cageid);

    let mut pipe_fds = [-1; 2];
    pipe(cageid, pipe_fds.as_mut_ptr());

    // Test closing read end
    let close_read_result = close(cageid, pipe_fds[0]);
    assert_eq!(close_read_result, 0, "closing read end failed");

    // Test closing write end
    let close_write_result = close(cageid, pipe_fds[1]);
    assert_eq!(close_write_result, 0, "closing write end failed");

    // Test double close (should fail)
    let double_close_result = close(cageid, pipe_fds[0]);
    assert!(double_close_result < 0, "double close should fail");

    // Test closing invalid fd
    let invalid_fd_result = close(cageid, 99999);
    assert!(invalid_fd_result < 0, "closing invalid fd should fail");

    testing_remove_all();
}

/// Test open syscall behavior
#[test]
fn test_open() {
    let cageid = 54;
    simple_init_cage(cageid);

    // Create a test file path in /tmp.
    let test_file = CString::new("/tmp/test_open_syscall_file").expect("CString::new failed");

    // Invoke open_syscall with flags O_CREAT | O_RDWR and mode 0o644.
    let fd = open_syscall(
        cageid,
        test_file.as_ptr() as u64,
        cageid,
        (libc::O_CREAT | libc::O_RDWR) as u64,
        cageid,
        0o644 as u64,
        cageid,
        0,
        0,
        0,
        0,
        0,
        0,
    );
    assert!(
        fd >= 0,
        "open_syscall should return a valid file descriptor, got: {}",
        fd
    );

    // Verify that the file was created.
    let metadata = fs::metadata("/tmp/test_open_syscall_file");
    assert!(metadata.is_ok(), "File should exist after open_syscall");

    // Clean up: remove the created test file.
    let _ = fs::remove_file("/tmp/test_open_syscall_file");

    testing_remove_all();
}

/// Test mkdir syscall behavior
#[test]
fn test_mkdir() {
    let cageid = 55;
    simple_init_cage(cageid);

    // Create a test directory path in /tmp.
    let test_dir = CString::new("/tmp/test_mkdir_syscall_dir").expect("CString::new failed");

    // Invoke mkdir_syscall with mode 0o755.
    let res = mkdir_syscall(
        cageid,
        test_dir.as_ptr() as u64,
        cageid,
        0o755 as u64,
        cageid,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    );
    assert_eq!(
        res, 0,
        "mkdir_syscall should succeed and return 0, got: {}",
        res
    );

    // Verify that the directory was created.
    let metadata = fs::metadata("/tmp/test_mkdir_syscall_dir");
    assert!(
        metadata.is_ok(),
        "Directory should exist after mkdir_syscall"
    );

    // Clean up: remove the created test directory.
    let _ = fs::remove_dir("/tmp/test_mkdir_syscall_dir");

    testing_remove_all();
}

#[test]
fn test_chdir() {
    // Initialize a test cage
    let cageid = 100;
    simple_init_cage(cageid);

    // Create a test directory first
    let test_dir = "/tmp/test_chdir_dir";
    let test_dir_cstring = CString::new(test_dir).unwrap();
    let test_dir_ptr = test_dir_cstring.as_ptr() as u64;

    // Create the directory
    let mkdir_result = mkdir_syscall(
        cageid,
        test_dir_ptr,
        cageid,
        0o755, // mode
        cageid,
        0, 0, 0, 0, 0, 0, 0, 0,
    );
    assert_eq!(mkdir_result, 0, "Failed to create test directory");

    // Test changing to the directory
    let chdir_result = chdir_syscall(
        cageid,
        test_dir_ptr,
        cageid,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    );
    assert_eq!(chdir_result, 0, "Failed to change directory");

    // Test changing to a non-existent directory
    let nonexistent_dir = "/tmp/nonexistent_dir_for_test";
    let nonexistent_dir_cstring = CString::new(nonexistent_dir).unwrap();
    let nonexistent_dir_ptr = nonexistent_dir_cstring.as_ptr() as u64;

    let chdir_error_result = chdir_syscall(
        cageid,
        nonexistent_dir_ptr,
        cageid,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    );
    assert!(chdir_error_result < 0, "Should fail when changing to non-existent directory");

    // Clean up
    unsafe {
        libc::rmdir(test_dir_cstring.as_ptr());
    }
}
