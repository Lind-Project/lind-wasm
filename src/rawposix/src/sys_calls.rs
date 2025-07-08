//! System syscalls implementation
//!
//! This module contains all system calls that are being emulated/faked in Lind.
use crate::fs_calls::kernel_close;
use cage::memory::mem_helper::*;
use cage::memory::vmmap::{VmmapOps, *};
use cage::{cagetable_init, add_cage, cagetable_clear, get_cage, remove_cage, Cage, Zombie};
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
    println!("cage id, {}", cageid);
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
    println!("[rawposix|waitpid] cp-3");
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
