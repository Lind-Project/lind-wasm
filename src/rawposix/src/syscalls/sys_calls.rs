//! System syscalls implementation
//!
//! This module contains all system calls that are being emulated/faked in Lind.
use crate::syscalls::fs_calls::kernel_close;
use cage::memory::mem_helper::*;
use cage::memory::vmmap::{VmmapOps, *};
use cage::{add_cage, cagetable_clear, get_cage, remove_cage, Cage, Zombie};
use fdtables;
use parking_lot::RwLock;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::Ordering::*;
use std::sync::atomic::{AtomicI32, AtomicU64};
use std::sync::Arc;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::fs_const::*;
use sysdefs::constants::{EXIT_SUCCESS, VERBOSE};
use typemap::syscall_conv::*;

/// Reference to Linux: https://man7.org/linux/man-pages/man2/fork.2.html
///
/// `fork_syscall` creates a new process (cage object). The newly created child process is an exact copy of the
/// parent process (the process that calls fork) apart from it's cage_id and the parent_id
/// In this function we separately handle copying fd tables and clone vmmap talbe and create a new Cage object
/// with this cloned tables.
pub fn fork_syscall(
    cageid: u64,
    child_arg: u64,        // Child's cage id
    child_arg_cageid: u64, // Child's cage id arguments cageid
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fork", "Invalide Cage ID");
    }

    // Modify the fdtable manually
    fdtables::copy_fdtable_for_cage(child_arg_cageid, child_arg).unwrap();

    // Get the self cage
    let selfcage = get_cage(child_arg_cageid).unwrap();

    let parent_vmmap = selfcage.vmmap.read();
    let new_vmmap = parent_vmmap.clone();

    let cageobj = Cage {
        cageid: child_arg,
        cwd: RwLock::new(selfcage.cwd.read().clone()),
        parent: child_arg_cageid,
        gid: AtomicI32::new(selfcage.gid.load(Relaxed)),
        uid: AtomicI32::new(selfcage.uid.load(Relaxed)),
        egid: AtomicI32::new(selfcage.egid.load(Relaxed)),
        euid: AtomicI32::new(selfcage.euid.load(Relaxed)),
        main_threadid: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(new_vmmap),
    };

    // increment child counter for parent
    selfcage.child_num.fetch_add(1, SeqCst);

    add_cage(child_arg, cageobj);
    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/exit.3.html
///
/// The exit function causes normal process(Cage) termination
/// The termination entails unmapping all memory references
/// Removing the cage object from the cage table, closing all open files which is removing corresponding fdtable
pub fn exit_syscall(
    cageid: u64,
    status_arg: u64,
    status_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let status = sc_convert_sysarg_to_i32(status_arg, status_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "exit", "Invalide Cage ID");
    }

    let _ = fdtables::remove_cage_from_fdtable(status_cageid);

    // Get the self cage
    let selfcage = get_cage(status_cageid).unwrap();
    if selfcage.parent != cageid {
        let parent_cage = get_cage(selfcage.parent);
        if let Some(parent) = parent_cage {
            parent.child_num.fetch_sub(1, SeqCst);
            let mut zombie_vec = parent.zombies.write();
            zombie_vec.push(Zombie {
                cageid: status_cageid,
                exit_code: status,
            });
        } else {
            // if parent already exited
            // BUG: we currently do not handle the situation where a parent has exited already
        }
    }

    status
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/exec.3.html
///
/// In our implementation, WASM is responsible for handling functionalities such as loading and executing
/// the new program, preserving process attributes, and resetting memory and the stack.
///
/// In RawPOSIX, the focus is on memory management inheritance and resource cleanup and release. Specifically,
/// RawPOSIX handles tasks such as clearing memory mappings, resetting shared memory, managing file descriptors
/// (closing or inheriting them based on the `should_cloexec` flag in fdtable), resetting semaphores, and
/// managing process attributes and threads (terminating unnecessary threads). This allows us to fully implement
/// the exec functionality while aligning with POSIX standards. Cage fields remained in exec():
/// cageid, cwd, parent, interval_timer
pub fn exec_syscall(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "exec", "Invalide Cage ID");
    }

    // Empty fd with flag should_cloexec
    fdtables::empty_fds_for_exec(cageid);

    // Copy necessary data from current cage
    let selfcage = get_cage(cageid).unwrap();

    let zombies = selfcage.zombies.read();
    let cloned_zombies = zombies.clone();
    let child_num = selfcage.child_num.load(Relaxed);
    drop(zombies);

    let newcage = Cage {
        cageid: cageid,
        cwd: RwLock::new(selfcage.cwd.read().clone()),
        parent: selfcage.parent,
        gid: AtomicI32::new(-1),
        uid: AtomicI32::new(-1),
        egid: AtomicI32::new(-1),
        euid: AtomicI32::new(-1),
        main_threadid: AtomicU64::new(0),
        zombies: RwLock::new(cloned_zombies), // When a process exec-ed, its child relationship should be perserved
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(Vmmap::new()), // Memory is cleared after exec
    };

    // Remove the original cage
    remove_cage(cageid);
    // Insert the new cage with same cageid
    add_cage(cageid, newcage);
    0
}

pub fn getpid_syscall(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "exec", "Invalide Cage ID");
    }

    let cage = get_cage(cageid).unwrap();

    return cage.cageid as i32;
}

pub fn getppid_syscall(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "exec", "Invalide Cage ID");
    }

    let cage = get_cage(cageid).unwrap();

    return cage.parent as i32;
}

/// Those functions are required by wasmtime to create the first cage. `verbosity` indicates whether
/// detailed error messages will be printed if set
pub fn lindrustinit(verbosity: isize) {
    let _ = VERBOSE.set(verbosity); //assigned to suppress unused result warning

    fdtables::register_close_handlers(FDKIND_KERNEL, fdtables::NULL_FUNC, kernel_close);

    let utilcage = Cage {
        cageid: 0,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        parent: 0,
        gid: AtomicI32::new(-1),
        uid: AtomicI32::new(-1),
        egid: AtomicI32::new(-1),
        euid: AtomicI32::new(-1),
        main_threadid: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(Vmmap::new()),
    };

    add_cage(
        0, // cageid
        utilcage,
    );
    fdtables::init_empty_cage(0);
    // Set the first 3 fd to STDIN / STDOUT / STDERR
    // TODO:
    // Replace the hardcoded values with variables (possibly by adding a LIND-specific constants file)
    let dev_null = CString::new("/home/lind-wasm/src/RawPOSIX/tmp/dev/null").unwrap();

    // Make sure that the standard file descriptor (stdin, stdout, stderr) is always valid, even if they
    // are closed before.
    // Standard input (fd = 0) is redirected to /dev/null
    // Standard output (fd = 1) is redirected to /dev/null
    // Standard error (fd = 2) is set to copy of stdout
    unsafe {
        libc::open(dev_null.as_ptr(), libc::O_RDONLY);
        libc::open(dev_null.as_ptr(), libc::O_WRONLY);
        libc::dup(1);
    }

    // STDIN
    fdtables::get_specific_virtual_fd(
        0,
        STDIN_FILENO as u64,
        FDKIND_KERNEL,
        STDIN_FILENO as u64,
        false,
        0,
    )
    .unwrap();
    // STDOUT
    fdtables::get_specific_virtual_fd(
        0,
        STDOUT_FILENO as u64,
        FDKIND_KERNEL,
        STDOUT_FILENO as u64,
        false,
        0,
    )
    .unwrap();
    // STDERR
    fdtables::get_specific_virtual_fd(
        0,
        STDERR_FILENO as u64,
        FDKIND_KERNEL,
        STDERR_FILENO as u64,
        false,
        0,
    )
    .unwrap();

    //init cage is its own parent
    let initcage = Cage {
        cageid: 1,
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

    // Add cage to cagetable
    add_cage(
        1, // cageid
        initcage,
    );

    fdtables::init_empty_cage(1);
    // Set the first 3 fd to STDIN / STDOUT / STDERR
    // STDIN
    fdtables::get_specific_virtual_fd(
        1,
        STDIN_FILENO as u64,
        FDKIND_KERNEL,
        STDIN_FILENO as u64,
        false,
        0,
    )
    .unwrap();
    // STDOUT
    fdtables::get_specific_virtual_fd(
        1,
        STDOUT_FILENO as u64,
        FDKIND_KERNEL,
        STDOUT_FILENO as u64,
        false,
        0,
    )
    .unwrap();
    // STDERR
    fdtables::get_specific_virtual_fd(
        1,
        STDERR_FILENO as u64,
        FDKIND_KERNEL,
        STDERR_FILENO as u64,
        false,
        0,
    )
    .unwrap();
}

pub fn lindrustfinalize() {
    let exitvec = cagetable_clear();

    for cageid in exitvec {
        exit_syscall(
            cageid as u64,       // target cageid
            EXIT_SUCCESS as u64, // status arg
            cageid as u64,       // status arg's cageid
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );
    }
}
