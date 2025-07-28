//! System syscalls implementation
//!
//! This module contains all system calls that are being emulated/faked in Lind.
use crate::fs_calls::kernel_close;
use cage::memory::mem_helper::*;
use cage::memory::vmmap::{VmmapOps, *};
use cage::{cagetable_init, add_cage, cagetable_clear, get_cage, remove_cage, Cage, Zombie, convert_signal_mask};
use fdtables;
use libc::sched_yield;
use parking_lot::RwLock;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::Ordering::*;
use std::sync::atomic::{AtomicI32, AtomicU64};
use std::sync::Arc;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::fs_const::{STDERR_FILENO, STDOUT_FILENO, STDIN_FILENO, *};
use sysdefs::constants::{EXIT_SUCCESS, VERBOSE};
use typemap::syscall_type_conversion::*;
use typemap::fs_type_conversion::*;
use dashmap::DashMap;
use typemap::{sc_convert_addr_to_host, sc_convert_sysarg_to_i32, sc_convert_sysarg_to_i32_ref, sc_unusedarg, sc_convert_buf_to_host, get_sockaddr, sc_convert_sysarg_to_u32};


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
        return syscall_error(Errno::EFAULT, "fork", "Invalide Arguments");
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
        main_threadid: RwLock::new(0),
        epoch_handler: DashMap::new(),
        pending_signals: RwLock::new(vec![]),
        signalhandler: selfcage.signalhandler.clone(),
        sigset: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num:  AtomicU64::new(0),
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
        return syscall_error(Errno::EFAULT, "exit", "Invalide Arguments");
    }

    let _ = fdtables::remove_cage_from_fdtable(cageid);
    
    // Get the self cage
    //may not be removable in case of lindrustfinalize, we don't unwrap the remove result
    if let Some(selfcage) = get_cage(cageid) {
        if selfcage.parent != cageid {
            let parent_cage = get_cage(selfcage.parent);
            if let Some(parent) = parent_cage {
                parent.child_num.fetch_sub(1, SeqCst);
                let mut zombie_vec = parent.zombies.write();
                zombie_vec.push(Zombie {
                    cageid: cageid,
                    exit_code: status,
                });
            } else {
                // if parent already exited
                // BUG: we currently do not handle the situation where a parent has exited already
            }
        }

        remove_cage(cageid);
    }
    
    status
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/waitpid.3p.html
///
/// waitpid() will return the cageid of waited cage, or 0 when WNOHANG is set and there is no cage already exited
/// waitpid_syscall utilizes the zombie list stored in cage struct. When a cage exited, a zombie entry will be inserted
/// into the end of its parent's zombie list. Then when parent wants to wait for any of child, it could just check its
/// zombie list and retrieve the first entry from it (first in, first out).
pub fn waitpid_syscall(
    cageid: u64,
    cageid_arg: u64,
    cageid_arg_cageid: u64,
    status_arg: u64,
    status_cageid: u64,
    options_arg: u64,
    options_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let status = sc_convert_sysarg_to_i32_ref(status_arg, status_cageid, cageid);
    let options = sc_convert_sysarg_to_i32(options_arg, options_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "waitpid", "Invalid Arguments");
    }

    // get the cage instance
    let cage = get_cage(cageid).unwrap();

    let mut zombies = cage.zombies.write();
    let child_num = cage.child_num.load(Relaxed);

    // if there is no pending zombies to wait, and there is no active child, return ECHILD
    if zombies.len() == 0 && child_num == 0 {
        return syscall_error(
            Errno::ECHILD,
            "waitpid",
            "no existing unwaited-for child processes",
        );
    }

    let mut zombie_opt: Option<Zombie> = None;

    // cageid <= 0 means wait for ANY child
    // cageid < 0 actually refers to wait for any child process whose process group ID equals -pid
    // but we do not have the concept of process group in lind, so let's just treat it as cageid == 0
    if cageid_arg <= 0 {
        loop {
            if zombies.len() == 0 && (options & libc::WNOHANG > 0) {
                // if there is no pending zombies and WNOHANG is set
                // return immediately
                return 0;
            } else if zombies.len() == 0 {
                // if there is no pending zombies and WNOHANG is not set
                // then we need to wait for children to exit
                // drop the zombies list before sleep to avoid deadlock
                drop(zombies);
                // TODO: replace busy waiting with more efficient mechanism
                unsafe {
                    sched_yield();
                }
                // after sleep, get the write access of zombies list back
                zombies = cage.zombies.write();
                continue;
            } else {
                // there are zombies avaliable
                // let's retrieve the first zombie
                zombie_opt = Some(zombies.remove(0));
                break;
            }
        }
    }
    // if cageid is specified, then we need to look up the zombie list for the id
    else {
        // first let's check if the cageid is in the zombie list
        if let Some(index) = zombies
            .iter()
            .position(|zombie| zombie.cageid == cageid_arg as u64)
        {
            // find the cage in zombie list, remove it from the list and break
            zombie_opt = Some(zombies.remove(index));
        } else {
            // if the cageid is not in the zombie list, then we know either
            // 1. the child is still running, or
            // 2. the cage has exited, but it is not the child of this cage, or
            // 3. the cage does not exist
            // we need to make sure the child is still running, and it is the child of this cage
            let child = get_cage(cageid_arg as u64);
            if let Some(child_cage) = child {
                // make sure the child's parent is correct
                if child_cage.parent != cage.cageid {
                    return syscall_error(
                        Errno::ECHILD,
                        "waitpid",
                        "waited cage is not the child of the cage",
                    );
                }
            } else {
                // cage does not exist
                return syscall_error(Errno::ECHILD, "waitpid", "cage does not exist");
            }

            // now we have verified that the cage exists and is the child of the cage
            loop {
                // the cage is not in the zombie list
                // we need to wait for the cage to actually exit

                // drop the zombies list before sleep to avoid deadlock
                drop(zombies);
                // TODO: replace busy waiting with more efficient mechanism
                unsafe {
                    sched_yield();
                }
                // after sleep, get the write access of zombies list back
                zombies = cage.zombies.write();

                // let's check if the zombie list contains the cage
                if let Some(index) = zombies
                    .iter()
                    .position(|zombie| zombie.cageid == cageid_arg as u64)
                {
                    // find the cage in zombie list, remove it from the list and break
                    zombie_opt = Some(zombies.remove(index));
                    break;
                }

                continue;
            }
        }
    }

    // reach here means we already found the desired exited child
    let zombie = zombie_opt.unwrap();
    // update the status
    *status = zombie.exit_code;
    
    // return child's cageid
    zombie.cageid as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/wait.2.html
///
/// See comments of waitpid_syscall
pub fn wait_syscall(
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
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "waitpid", "Invalid Arguments");
    }
    // left type conversion done inside waitpid_syscall
    waitpid_syscall(
        cageid,
        0,
        0,
        status_arg,
        status_cageid,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )
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
    cagetable_init();

    fdtables::register_close_handlers(FDKIND_KERNEL, fdtables::NULL_FUNC, kernel_close);

    let utilcage = Cage {
        cageid: 0,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        parent: 0,
        gid: AtomicI32::new(-1),
        uid: AtomicI32::new(-1),
        egid: AtomicI32::new(-1),
        euid: AtomicI32::new(-1),
        main_threadid: RwLock::new(0),
        epoch_handler: DashMap::new(),
        pending_signals: RwLock::new(vec![]),
        signalhandler: DashMap::new(),
        sigset: AtomicU64::new(0),
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
        main_threadid: RwLock::new(0),
        epoch_handler: DashMap::new(),
        signalhandler: DashMap::new(),
        pending_signals: RwLock::new(vec![]),
        sigset: AtomicU64::new(0),
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
        main_threadid: RwLock::new(0),
        epoch_handler: DashMap::new(),
        signalhandler: selfcage.signalhandler.clone(),
        pending_signals: RwLock::new(vec![]),
        sigset: AtomicU64::new(0),
        zombies: RwLock::new(cloned_zombies), // When a process exec-ed, its child relationship should be perserved
        child_num: AtomicU64::new(child_num),
        vmmap: RwLock::new(Vmmap::new()), // Memory is cleared after exec
    };

    // Remove the original cage
    remove_cage(cageid);
    // Insert the new cage with same cageid
    add_cage(cageid, newcage);
    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/send.2.html
///
/// The Linux `send()` syscall is used to transmit a message through a socket.
/// This implementation extracts the virtual file descriptor and buffer from the current cage,
/// then forwards the request to the host kernel with the provided flags.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor indicating the socket to send data on
///     - buf_arg: pointer to the message buffer in user memory
///     - buflen_arg: length of the message to be sent
///     - flags_arg: bitmask of flags influencing message transmission behavior
///
/// Return:
///     - On success: number of bytes sent
///     - On failure: a negative errno value indicating the syscall error
pub fn send_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = sc_convert_buf_to_host(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "send_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::send(fd as i32, buf as *const c_void, buflen, flags) as i32};
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "send");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/recv.2.html
///
/// The Linux `recv()` syscall is used to receive a message from a connected socket.
/// This implementation retrieves the virtual file descriptor and target buffer from the current cage,
/// and performs the message receive operation using the specified flags.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor from which to receive data
///     - buf_arg: pointer to the buffer in user memory to store received data
///     - buflen_arg: size of the buffer to receive data into
///     - flags_arg: flags controlling message reception behavior
///
/// Return:
///     - On success: number of bytes received
///     - On failure: a negative errno value indicating the syscall error
pub fn recv_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = sc_convert_buf_to_host(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "recv_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::recv(fd, buf as *mut c_void, buflen, flags) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "recv");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/bind.2.html
///
/// The Linux `bind()` syscall assigns a local address to a socket, which is required before a socket
/// can accept incoming connections. This implementation first converts the virtual file descriptor and
/// socket address from the calling cage into kernel-visible forms. If the address is a UNIX domain
/// socket (AF_UNIX), the path is rewritten to include a sandbox root (`LIND_ROOT`) to enforce proper
/// isolation within the namespace.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor to be bound
///     - addr_arg: pointer to a `sockaddr_un` structure containing the local address
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn bind_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "bind_syscall", "Invalide Cage ID");
    }

    let (finalsockaddr, addrlen) = get_sockaddr(addr);

    let ret = unsafe { libc::bind(fd, finalsockaddr, addrlen) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "bind");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/connect.2.html
///
/// The Linux `connect()` syscall connects a socket referred to by a file descriptor to the specified
/// address. This implementation resolves the provided virtual file descriptor and memory address from
/// the calling cage and performs the corresponding kernel operation. If the socket is a UNIX domain
/// socket (AF_UNIX), the path is modified to include the sandbox root path (`LIND_ROOT`) to ensure the
/// socket file resides within the correct namespace.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor for the socket to be connected
///     - addr_arg: pointer to a `sockaddr_un` structure containing the target address
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn connect_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {

    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);
    
    if !(sc_unusedarg(arg3, arg3_cageid)
        &&sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "connect_syscall", "Invalide Cage ID");
    }
    
    let (finalsockaddr, addrlen) = get_sockaddr(addr);

    let ret = unsafe { libc::connect(fd, finalsockaddr, addrlen) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "connect");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/listen.2.html
///
/// The Linux `listen()` syscall marks a socket as passive, indicating that it will be used to accept
/// incoming connection requests. This implementation converts the virtual file descriptor and backlog
/// value from the calling cage to their kernel-visible equivalents, and invokes the system `listen()` call.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor referring to the socket
///     - backlog_arg: maximum number of pending connections in the socket’s listen queue
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn listen_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    backlog_arg: u64,
    backlog_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let backlog = sc_convert_sysarg_to_i32(backlog_arg, backlog_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "listen_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::listen(fd, backlog) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "listen");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/accept.2.html
///
/// The Linux `accept()` syscall extracts the first connection request on the queue of pending
/// connections for the listening socket, creates a new connected socket, and returns a new file descriptor
/// referring to that socket. In this implementation, we convert the virtual file descriptor to the host one,
/// and if provided, transform the sockaddr pointer for use inside the kernel. The returned host file
/// descriptor is then assigned a new virtual file descriptor.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor referring to the listening socket
///     - addr_arg: optional pointer to a buffer that will receive the address of the connecting entity
///     - len_arg: not used in this implementation
///
/// Return:
///     - On success: new virtual file descriptor associated with the accepted socket
///     - On failure: a negative errno value indicating the syscall error
pub fn accept_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "accept_syscall", "Invalide Cage ID");
    }

    let (finalsockaddr, mut addrlen) = get_sockaddr(addr);

    let ret_kernelfd = unsafe { libc::accept(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret_kernelfd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "accept");
    }

    let ret_virtualfd = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ret_kernelfd as u64, false, 0).unwrap();
    
    ret_virtualfd as i32

}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/setsockopt.2.html
///
/// The Linux `setsockopt()` syscall sets options for a socket. Options may exist at multiple protocol levels.
/// This implementation translates the virtual file descriptor and user-provided option values into host-space values
/// before applying the `setsockopt` syscall on the host kernel.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor representing the socket
///     - level_arg: specifies the protocol level at which the option resides (e.g., SOL_SOCKET)
///     - optname_arg: option name to be set (e.g., SO_REUSEADDR)
///     - optval_arg: pointer to the option value
///     - optlen_arg: size of the option value
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn setsockopt_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    level_arg: u64,
    level_cageid: u64,
    optname_arg: u64,
    optname_cageid: u64,
    optval_arg: u64,
    optval_cageid: u64,
    optlen_arg: u64,
    optlen_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let level = sc_convert_sysarg_to_i32(level_arg, level_cageid, cageid);
    let optname = sc_convert_sysarg_to_i32(optname_arg, optname_cageid, cageid);
    let optval = sc_convert_addr_to_host(optval_arg, optval_cageid, cageid);
    let optlen = sc_convert_sysarg_to_u32(optlen_arg, optlen_cageid, cageid);

    if !(sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "setsockopt_syscall", "Invalide Cage ID");
    }
    let ret = unsafe { 
        libc::setsockopt(fd, level, optname, optval as *mut c_void, optlen)
    };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "setsockopt");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/socket.2.html
///
/// The Linux `socket()` syscall creates an endpoint for communication and returns a file descriptor
/// for the newly created socket. This implementation wraps the syscall and registers the resulting
/// file descriptor in our virtual file descriptor table (`fdtables`) under the current cage.
///
/// The `fdtables` system manages per-cage file descriptors and tracks their lifecycle.
///
/// Input:
///     - cageid: current cageid
///     - domain_arg: communication domain (e.g., AF_INET, AF_UNIX)
///     - socktype_arg: socket type (e.g., SOCK_STREAM, SOCK_DGRAM)
///     - protocol_arg: protocol to be used (usually 0)
///
/// Return:
///     - On success: a newly allocated virtual file descriptor within the current cage
///     - On failure: a negative errno value indicating the syscall error
pub fn socket_syscall(
    cageid: u64,
    domain_arg: u64,
    domain_cageid: u64,
    socktype_arg: u64,
    socktype_cageid: u64,
    protocol_arg: u64,
    protocol_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {

    let domain = sc_convert_sysarg_to_i32(domain_arg, domain_cageid, cageid);
    let socktype = sc_convert_sysarg_to_i32(socktype_arg, socktype_cageid, cageid);
    let protocol = sc_convert_sysarg_to_i32(protocol_arg, protocol_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "socket_syscall", "Invalide Cage ID");
    }

    let kernel_fd = unsafe { libc::socket(domain, socktype, protocol) };
       
        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "socket");
        }

        return fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, false, 0).unwrap() as i32;
}

pub fn sigprocmask_syscall(
    cageid: u64,
    how_arg: u64,
    how_cageid: u64,
    set_arg: u64,
    set_cageid: u64,
    oldset_arg: u64,
    oldset_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let how = sc_convert_sysarg_to_i32(how_arg, how_cageid, cageid);
    let set = sc_convert_sigset(set_arg, set_cageid, cageid);
    let oldset = sc_convert_sigset(oldset_arg, oldset_cageid, cageid);
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "sigprocmask_syscall", "Invalide Cage ID");
    }

    let cage = get_cage(cageid).unwrap();

    let mut res = 0;

    if let Some(some_oldset) = oldset {
        *some_oldset = cage.sigset.load(Relaxed);
    }

    if let Some(some_set) = set {
        let curr_sigset = cage.sigset.load(Relaxed);
        res = match how {
            SIG_BLOCK => {
                // Block signals in set
                cage.sigset.store(
                    curr_sigset | *some_set,
                    Relaxed,
                );
                0
            }
            SIG_UNBLOCK => {
                // Unblock signals in set
                let newset = curr_sigset & !*some_set;
                cage.sigset
                    .store(newset, Relaxed);
                // check if any of the unblocked signals are in the pending signal list
                // and trigger the epoch if it has
                let pending_signals = cage.pending_signals.read();
                if pending_signals
                    .iter()
                    .any(|signo| (*some_set & convert_signal_mask(*signo)) != 0)
                {
                    cage::signal_epoch_trigger(cage.cageid);
                }
                0
            }
            SIG_SETMASK => {
                let pending_signals = cage.pending_signals.read();
                // find all signals switched from blocking to nonblocking
                // 1. perform a xor operation to find signals that switched state
                // all the signal masks changed from 0 to 1, or 1 to 0 are filtered in this step
                // 2. perform an and operation to the old sigset, this further filtered masks and only
                // left masks changed from 1 to 0
                let unblocked_signals = (curr_sigset ^ *some_set) & curr_sigset;
                // check if any of the unblocked signals are in the pending signal list
                // and trigger the epoch if it has
                if pending_signals
                    .iter()
                    .any(|signo| (unblocked_signals & convert_signal_mask(*signo)) != 0)
                {
                    cage::signal_epoch_trigger(cage.cageid);
                }
                // Set sigset to set
                cage.sigset
                    .store(*some_set, Relaxed);
                0
            }
            _ => syscall_error(Errno::EINVAL, "sigprocmask", "Invalid value for how"),
        }
    }
    res
}