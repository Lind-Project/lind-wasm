//! System syscalls implementation
//!
//! This module contains all system calls that are being emulated/faked in Lind.
use crate::fs_calls::kernel_close;
use cage::memory::vmmap::{VmmapOps, *};
use cage::signal::signal::{convert_signal_mask, lind_send_signal, signal_check_trigger};
use cage::timer::IntervalTimer;
use cage::{add_cage, cagetable_clear, cagetable_init, get_cage, remove_cage, Cage, Zombie};
use dashmap::DashMap;
use fdtables;
use libc::sched_yield;
use parking_lot::{Mutex, RwLock};
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::Ordering::*;
use std::sync::atomic::{AtomicI32, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno, VERBOSE};
use sysdefs::constants::fs_const::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use sysdefs::constants::lind_platform_const::{FDKIND_KERNEL, LIND_ROOT};
use sysdefs::constants::sys_const::{
    DEFAULT_GID, DEFAULT_UID, EXIT_SUCCESS, ITIMER_REAL, SIGCHLD, SIGKILL, SIGSTOP, SIG_BLOCK,
    SIG_SETMASK, SIG_UNBLOCK, WNOHANG,
};
use sysdefs::data::fs_struct::{ITimerVal, SigactionStruct};
use typemap::datatype_conversion::*;

/// Reference to Linux: https://man7.org/linux/man-pages/man2/fork.2.html
///
/// We implement `fork` in user space because a Cage is not a host kernel process,
/// and its granularity does not align with the host kernel’s notion of processes.
/// Delegating directly to the kernel would not allow us to capture the Cage-level
/// semantics we need. Instead, this function performs the resource management that
/// `fork` implies in our model: duplicating the file descriptor table, cloning the
/// virtual memory map, and constructing a new Cage object that mirrors the parent’s
/// state. In this way, we preserve the familiar fork semantics while keeping control
/// at the Cage abstraction level.
///
/// Actual operations of the address space is handled by wasmtime when creating a new
/// instance for the child cage.
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
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fork_syscall"
        );
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
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(child_arg),
        epoch_handler: DashMap::new(),
        pending_signals: RwLock::new(vec![]),
        signalhandler: selfcage.signalhandler.clone(),
        sigset: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(new_vmmap),
    };

    // increment child counter for parent
    selfcage.child_num.fetch_add(1, SeqCst);

    add_cage(child_arg, cageobj);
    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/exec.3.html
///
/// In our implementation, Wasmtime is responsible for handling functionalities such as loading and executing
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
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "exec_syscall"
        );
    }

    // Empty fd with flag should_cloexec
    fdtables::empty_fds_for_exec(cageid);

    // Copy necessary data from current cage
    let selfcage = get_cage(cageid).unwrap();

    selfcage.rev_shm.lock().clear();

    // ensures that all old mappings and states are discarded, allowing the new cage to
    // run in a clean virtual address space, while reusing the existing `Vmmap` container
    // to avoid extra allocations.
    let mut vmmap = selfcage.vmmap.write();
    vmmap.clear(); //todo: this just clean the vmmap in the cage, still need some modify for wasmtime and call to kernal

    // perform signal related clean up
    // all the signal handler becomes default after exec
    // pending signals should be perserved though
    selfcage.signalhandler.clear();
    // the sigset will be reset after exec
    selfcage.sigset.store(0, Relaxed);
    // we also clean up epoch handler and main thread id
    // since they will be re-established from wasmtime
    selfcage.epoch_handler.clear();
    let mut threadid_guard = selfcage.main_threadid.write();
    *threadid_guard = 0;
    drop(threadid_guard);

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
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "exit_syscall"
        );
    }

    // Cleanup fdtable
    let _ = fdtables::remove_cage_from_fdtable(cageid);

    // Cleanup cage table
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

        // if the cage has parent (i.e. it is not the "root" cage)
        if cageid != selfcage.parent {
            lind_send_signal(selfcage.parent, SIGCHLD);
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
    let status = {
        if status_arg == 0 {
            None
        } else {
            Some(sc_convert_sysarg_to_i32_ref(
                status_arg,
                status_cageid,
                cageid,
            ))
        }
    };
    let options = sc_convert_sysarg_to_i32(options_arg, options_cageid, cageid);
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "waitpid_syscall"
        );
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
            if zombies.len() == 0 && (options & WNOHANG > 0) {
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
                // Check for pending signals after yielding (only if WNOHANG is not set)
                if (options & WNOHANG == 0) && signal_check_trigger(cage.cageid) {
                    return syscall_error(Errno::EINTR, "waitpid", "interrupted by signal");
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
                // Check for pending signals after yielding (only if WNOHANG is not set)
                if (options & WNOHANG == 0) && signal_check_trigger(cage.cageid) {
                    return syscall_error(Errno::EINTR, "waitpid", "interrupted by signal");
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
    if let Some(status) = status {
        *status = zombie.exit_code;
    }

    // return child's cageid
    zombie.cageid as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getpid.2.html
///
/// Implements `getpid`.  
/// In our model, a Cage’s `pid` is simply its `cageid`, stored locally in the Cage
/// structure rather than managed by the host kernel. This allows each Cage to
/// behave like a process with its own identifier while remaining within the
/// user-space runtime.
///
/// ## Returns
/// Get the parent cage ID
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
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getpid_syscall"
        );
    }

    let cage = get_cage(cageid).unwrap();

    return cage.cageid as i32;
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/getppid.3p.html
///
/// See comments of `getpid_syscall` for more details
///
/// ## Returns
/// Get the parent cage ID
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
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getppid", "invalid Cage ID");
    }

    let cage = get_cage(cageid).unwrap();

    return cage.parent as i32;
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getgid.2.html
///
/// Get the real **host** group ID of the calling process.
///
/// ## Returns
/// These functions are always successful and never modify errno.
pub fn getgid_syscall(
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
    // Validate that unused arguments are indeed unused.
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getgid_syscall"
        );
    }

    (unsafe { libc::getgid() }) as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getegid.2.html
///
/// Get the effective **host** group ID of the calling process.
///
/// ## Returns
/// These functions are always successful and never modify errno.
pub fn getegid_syscall(
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
    // Validate that all extra arguments are unused.
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getegid_syscall"
        );
    }

    (unsafe { libc::getegid() }) as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getuid.2.html
///
/// Get the real **host** user ID of the calling process.
///
/// ## Returns
/// These functions are always successful and never modify errno.
pub fn getuid_syscall(
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
    // Validate unused arguments.
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getuid_syscall"
        );
    }

    (unsafe { libc::getuid() }) as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/geteuid.2.html
///
/// Get the real **host** effective ID of the calling process.
///
/// ## Returns
/// These functions are always successful and never modify errno.
pub fn geteuid_syscall(
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
    // Validate that each extra argument is unused.
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "geteuid_syscall"
        );
    }

    (unsafe { libc::geteuid() }) as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/sigaction.2.html
///
/// Copy the existing signal handler state from the cage into the caller-provided memory
/// (oact). Install the new handler provided by the caller into the cage’s signal handler
/// table (act). Reserved arguments must remain unused, and SIGKILL/SIGSTOP cannot be
/// modified.
///
/// # Arguments
/// * `cageid` - The ID of the cage invoking the syscall.
/// * `sig_arg` - Signal number (as u64, later cast to i32).
/// * `sig_arg_cageid` - Cage ID that owns the `sig_arg` (for validation).
/// * `act_arg` - Pointer to the new `sigaction` struct, or 0 if none.
/// * `act_arg_cageid` - Cage ID of the memory holding `act_arg`.
/// * `oact_arg` - Pointer to store the old `sigaction` struct, or 0 if not needed.
/// * `oact_arg_cageid` - Cage ID of the memory holding `oact_arg`.
///
/// # Returns
/// * `0` on success.
/// * Negative errno wrapped via `syscall_error` on failure.
pub fn sigaction_syscall(
    cageid: u64,
    sig_arg: u64,
    sig_arg_cageid: u64,
    act_arg: u64,
    act_arg_cageid: u64,
    oact_arg: u64,
    oact_arg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let sig = sc_convert_sysarg_to_i32(sig_arg, sig_arg_cageid, cageid);
    let act = sc_convert_sigactionStruct(act_arg, act_arg_cageid, cageid);
    let oact = sc_convert_sigactionStruct_mut(oact_arg, oact_arg_cageid, cageid);
    // Validate that the extra unused arguments are indeed unused.
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "sigaction_syscall"
        );
    }

    // Retrieve the cage.
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => return syscall_error(Errno::ECHILD, "sigaction", "Cage not found"),
    };

    // If oact (old action pointer) is provided, fill it with the current action.
    if let Some(oact_ref) = oact {
        if let Some(current_act) = cage.signalhandler.get(&sig) {
            // Copy the current signal action into the provided memory.
            oact_ref.clone_from(&current_act);
        } else {
            // If there is no current action, use a default.
            oact_ref.clone_from(&SigactionStruct::default());
        }
    }

    // If a new action is provided in act, update the signal handler.
    if let Some(new_act) = act {
        // Disallow modification for SIGKILL and SIGSTOP.
        if sig == SIGKILL || sig == SIGSTOP {
            return syscall_error(
                Errno::EINVAL,
                "sigaction",
                "Cannot modify SIGKILL or SIGSTOP",
            );
        }
        // Insert the new signal action into the cage’s signal handler table.
        cage.signalhandler.insert(sig, new_act.clone());
    }

    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/kill.2.html
///
/// This function allows one cage (the caller) to send a signal to another cage
/// (the target), similar to the `kill(2)` syscall in POSIX.
///
/// ## Arguments
/// * `cageid` - The ID of the calling cage (not directly used to deliver the signal).
/// * `target_cage_arg` / `target_cage_arg_cageid` - Encoded system arguments
///   specifying the target cage ID to which the signal should be sent.
/// * `sig_arg` / `sig_arg_cageid` - Encoded system arguments specifying the signal number.
///
/// ## Returns
/// On success, returns `0`. If the target cage does not exist, returns `ESRCH`.
///
/// ## Errors
/// * `EFAULT` – Reserved arguments were not unused.
/// * `EINVAL` – Invalid target cage ID or signal number.
/// * `ESRCH` – Target cage does not exist.
pub fn kill_syscall(
    cageid: u64,
    target_cage_arg: u64,
    target_cage_arg_cageid: u64,
    sig_arg: u64,
    sig_arg_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Convert target cage id and signal value.
    let target_cage = sc_convert_sysarg_to_i32(target_cage_arg, target_cage_arg_cageid, cageid);
    let sig = sc_convert_sysarg_to_i32(sig_arg, sig_arg_cageid, cageid);

    // Validate the unused arguments.
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "kill_syscall"
        );
    }

    // Validate the target cage id: it must not be negative and typically within a system-defined maximum.
    if target_cage < 0 {
        return syscall_error(Errno::EINVAL, "kill", "Invalid target cage id");
    }

    // Validate the signal number: for example, it should typically be in the range 1..32.
    if sig <= 0 || sig >= 32 {
        return syscall_error(Errno::EINVAL, "kill", "Invalid signal number");
    }

    // If pid equals 0, then sig is sent to every process in the process
    // group of the calling process.
    // As we do not have the concept of process group, we just send the signal
    // to itself
    let target_cage = {
        if target_cage == 0 {
            cageid
        } else {
            target_cage as u64
        }
    };

    // Optionally, you could verify that certain signals (e.g., SIGKILL, SIGSTOP)
    // are handled with special semantics; however, in this implementation we assume they are valid.

    // Attempt to send the signal using a helper function such as lind_send_signal.
    // This helper returns a boolean indicating whether the operation was successful.
    // The caller's cage id is not directly used to send the signal; instead, the target cage id is used.
    if !lind_send_signal(target_cage as u64, sig) {
        return syscall_error(Errno::ESRCH, "kill", "Target cage does not exist");
    }

    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/sigprocmask.2.html
///
/// This function allows a cage to examine or change its
/// signal mask, i.e., the set of signals currently blocked from delivery.
/// If `oldset` is provided, copies the current signal mask into it. If `set` is
/// provided, updates the mask according to `how`:
///    - `SIG_BLOCK`: add signals from `set` to the mask.
///    - `SIG_UNBLOCK`: remove signals from `set` from the mask; if any pending
///       signals are now unblocked, trigger a signal epoch.
///    - `SIG_SETMASK`: replace the mask with `set`; if any previously blocked
///       pending signals are now unblocked, trigger a signal epoch.
///
/// ## Arguments
/// * `cageid` – The ID of the calling cage.
/// * `how_arg` / `how_cageid` – Encoded argument specifying how the mask is modified
///   (`SIG_BLOCK`, `SIG_UNBLOCK`, or `SIG_SETMASK`).
/// * `set_arg` / `set_cageid` – Optional pointer to a new signal set.
///   - If provided, defines the signals to block, unblock, or set.
///   - If null, the mask is not modified.
/// * `oldset_arg` / `oldset_cageid` – Optional pointer where the previous mask
///   should be stored.
///
/// ## Returns:
/// Returns `0` on success, or an error code (`EINVAL`, `EFAULT`) on failure.
///
/// ## Errors
/// * `EFAULT` – Reserved arguments were not unused.
/// * `EINVAL` – Invalid value passed for `how`.
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
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "sigprocmask_syscall"
        );
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
                cage.sigset.store(curr_sigset | *some_set, Relaxed);
                0
            }
            SIG_UNBLOCK => {
                // Unblock signals in set
                let newset = curr_sigset & !*some_set;
                cage.sigset.store(newset, Relaxed);
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
                cage.sigset.store(*some_set, Relaxed);
                0
            }
            _ => syscall_error(Errno::EINVAL, "sigprocmask", "Invalid value for how"),
        }
    }
    res
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/sched_yield.2.html
///
/// Causes the calling thread to relinquish the CPU. The thread is moved to the end
/// of the queue for its static priority and a new thread gets to run.
///
/// ## Returns
/// On success, returns 0. On error, -1 is returned, and errno is set.
pub fn sched_yield_syscall(
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
    // Validate that each extra argument is unused.
    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "sched_yield_syscall"
        );
    }

    (unsafe { sched_yield() }) as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/setitimer.3p.html
///
/// This syscall allows a cage to set or retrieve the value of an interval timer.
/// Currently only `ITIMER_REAL` is supported, which decrements in real (wall-clock)
/// time and delivers `SIGALRM` upon expiration.
///
/// For `ITIMER_REAL`:  
///    - If `old_value` is provided, copies the current timer’s remaining time and interval
///      into it.  
///    - If `new_value` is provided, updates the interval timer with the new durations.  
/// For `ITIMER_VIRTUAL` and `ITIMER_PROF`, no action is taken (not implemented).
///
/// ## Arguments
/// * `cageid` – The ID of the calling cage.
/// * `which_arg` / `which_arg_cageid` – Encoded argument specifying which timer to use
///   (`ITIMER_REAL`, `ITIMER_VIRTUAL`, `ITIMER_PROF`). Only `ITIMER_REAL` is implemented.
/// * `new_value_arg` / `new_value_arg_cageid` – Pointer to a new `itimerval` struct.
///   If non-null, this specifies the new timer settings.
/// * `old_value_arg` / `old_value_arg_cageid` – Pointer to an `itimerval` struct
///   where the previous timer value should be stored. If non-null, the current
///   timer is copied here before being changed.
///
/// ## Returns
/// * `0` on success.
/// * Negative errno (`EFAULT`, etc.) on failure.
pub fn setitimer_syscall(
    cageid: u64,
    which_arg: u64,
    which_arg_cageid: u64,
    new_value_arg: u64,
    new_value_arg_cageid: u64,
    old_value_arg: u64,
    old_value_arg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let which = sc_convert_sysarg_to_i32(which_arg, which_arg_cageid, cageid);
    let new_value = sc_convert_itimerval(new_value_arg, new_value_arg_cageid, cageid);
    let old_value = sc_convert_itimerval_mut(old_value_arg, old_value_arg_cageid, cageid);
    // Validate that extra arguments are indeed unused.
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "setitimer_syscall"
        );
    }

    // get the cage instance
    let cage = get_cage(cageid).unwrap();

    match which {
        ITIMER_REAL => {
            if let Some(some_old_value) = old_value {
                let (curr_duration, next_duration) = cage.interval_timer.get_itimer();
                some_old_value.it_value.tv_sec = curr_duration.as_secs() as i64;
                some_old_value.it_value.tv_usec = curr_duration.subsec_millis() as i64;
                some_old_value.it_interval.tv_sec = next_duration.as_secs() as i64;
                some_old_value.it_interval.tv_usec = next_duration.subsec_millis() as i64;
            }

            if let Some(some_new_value) = new_value {
                let curr_duration = Duration::new(
                    some_new_value.it_value.tv_sec as u64,
                    some_new_value.it_value.tv_usec as u32,
                );
                let next_duration = Duration::new(
                    some_new_value.it_interval.tv_sec as u64,
                    some_new_value.it_interval.tv_usec as u32,
                );

                cage.interval_timer.set_itimer(curr_duration, next_duration);
            }
        }

        _ => { /* ITIMER_VIRTUAL and ITIMER_PROF is not implemented*/ }
    }
    0
}

/// Those functions are required by wasmtime to create the first cage. `verbosity` indicates whether
/// detailed error messages will be printed if set
pub fn rawposix_start(verbosity: isize) {
    let _ = VERBOSE.set(verbosity); //assigned to suppress unused result warning
    cagetable_init();

    fdtables::register_close_handlers(FDKIND_KERNEL, fdtables::NULL_FUNC, kernel_close);

    // Set up standard file descriptors for the init cage
    // TODO:
    // Replace the hardcoded values with variables (possibly by adding a LIND-specific constants file)
    let dev_null = CString::new(format!("{}/dev/null", LIND_ROOT)).unwrap();

    // Make sure that the standard file descriptors (stdin, stdout, stderr) are always valid
    // Standard input (fd = 0) is redirected to /dev/null
    // Standard output (fd = 1) is redirected to /dev/null
    // Standard error (fd = 2) is set to copy of stdout
    unsafe {
        libc::open(dev_null.as_ptr(), libc::O_RDONLY);
        libc::open(dev_null.as_ptr(), libc::O_WRONLY);
        libc::dup(1);
    }

    //init cage is its own parent
    let initcage = Cage {
        cageid: 1,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        parent: 1,
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(1),
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

pub fn rawposix_shutdown() {
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
