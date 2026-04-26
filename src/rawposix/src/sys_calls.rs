//! System syscalls implementation
//!
//! This module contains all system calls that are being emulated/faked in Lind.
use crate::fs_calls::kernel_close;
use cage::memory::vmmap::{VmmapOps, *};
use cage::signal::signal::{convert_signal_mask, lind_send_signal, signal_check_trigger};
use cage::timer::IntervalTimer;
use cage::{add_cage, encode_wait_status, get_cage, remove_cage, Cage, ExitStatus, Zombie};
use dashmap::DashMap;
use fdtables;
use libc::sched_yield;
use parking_lot::{Mutex, RwLock};
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::Ordering::*;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use sysdefs::constants::err_const::{syscall_error, Errno, VERBOSE};
use sysdefs::constants::fs_const::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use sysdefs::constants::lind_platform_const::{
    MAX_CAGEID, MAX_LINEAR_MEMORY_SIZE, RAWPOSIX_CAGEID, UNUSED_ARG, UNUSED_ID, UNUSED_NAME,
    WASMTIME_CAGEID,
};
use sysdefs::constants::sys_const::{
    DEFAULT_GID, DEFAULT_UID, EXIT_SUCCESS, ITIMER_REAL, RLIMIT_AS, RLIMIT_CORE, RLIMIT_DATA,
    RLIMIT_NOFILE, RLIMIT_NPROC, RLIMIT_RSS, RLIMIT_STACK, SIGCHLD, SIGKILL, SIGSTOP, SIG_BLOCK,
    SIG_SETMASK, SIG_UNBLOCK, WNOHANG,
};
use sysdefs::constants::syscall_const;
use sysdefs::data::fs_struct::{ITimerVal, Rlimit, SigactionStruct};
use sysdefs::logging::lind_debug_panic;
use sysdefs::{constants::sys_const, data::sys_struct};
use typemap::datatype_conversion::*;

/// Reference to Linux: https://man7.org/linux/man-pages/man2/clone.2.html
/// Reference to Linux: https://man7.org/linux/man-pages/man2/fork.2.html
///
/// ## Unified clone handling
///
/// In Linux, both `fork` and `pthread_create` are implemented as special
/// cases of the `clone` syscall, distinguished by the presence of
/// `CLONE_VM`. RawPOSIX follows the same model and exposes a unified
/// `clone` entry point, but splits responsibility along abstraction
/// boundaries.
///
/// RawPOSIX is responsible for **Cage-level structure**, not instruction-level
/// execution or address-space manipulation. As a result, RawPOSIX only expands
/// the **fork-like subset** of `clone` (i.e., when `CLONE_VM` is *not* set).
///
/// After performing any required Cage-level setup, control is transferred
/// back to Wasmtime via the 3i dispatch mechanism, where both fork and
/// thread creation semantics are completed.
///
/// ## fork
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
///
/// ## thread
///
/// When `CLONE_VM` *is* set, the operation corresponds to thread creation
/// (`pthread_create`). In this case, no new Cage is created. Threads share the same
/// Cage. Address space and execution context must be duplicated or adjusted within
/// the WebAssembly runtime.
pub extern "C" fn fork_syscall(
    cageid: u64,
    clone_arg: u64,        // Child's cage id
    clone_arg_cageid: u64, // Child's cage id arguments cageid
    parent_tid: u64,
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
    // Extract the ABI-level clone argument structure
    let args = unsafe { &mut *(clone_arg as *mut sys_struct::CloneArgStruct) };
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fork_syscall"
        );
    }

    // Determine clone semantics from flags.
    //
    // if CLONE_VM is set, we are creating a new thread (i.e. pthread_create)
    // otherwise, we are creating a process (i.e. fork)
    let flags = args.flags;
    let isthread = flags & (sys_const::CLONE_VM);

    // Effective parent cage ID.
    let parent_cageid = cageid;
    let mut child_cageid = 0;

    // Fork path: create a new cage
    if isthread == 0 {
        // Allocate a fresh cage ID for the child.
        child_cageid = cage::alloc_cage_id().unwrap();

        // Duplicate the parent's file descriptor table.
        fdtables::copy_fdtable_for_cage(parent_cageid, child_cageid).unwrap();

        // Get the self cage
        let selfcage = get_cage(parent_cageid).unwrap();

        // Clone the parent's virtual memory map
        let parent_vmmap = selfcage.vmmap.read();
        let new_vmmap = parent_vmmap.clone();

        // Creat the child cage object
        let cageobj = Cage {
            cageid: child_cageid,
            cwd: RwLock::new(selfcage.cwd.read().clone()),
            parent: parent_cageid,
            rev_shm: Mutex::new(Vec::new()),
            main_threadid: RwLock::new(0),
            interval_timer: IntervalTimer::new(child_cageid),
            epoch_handler: DashMap::new(),
            os_tid_map: DashMap::new(),
            pending_signals: RwLock::new(vec![]),
            signalhandler: selfcage.signalhandler.clone(),
            sigset: AtomicU64::new(0),
            zombies: RwLock::new(vec![]),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(new_vmmap),
            final_exit_status: RwLock::new(None),
            exit_group_initiated: AtomicBool::new(false),
            is_dead: AtomicBool::new(false),
            grate_inflight: AtomicU64::new(0),
        };

        // increment child counter for parent
        selfcage.child_num.fetch_add(1, SeqCst);

        // Register the new cage to global cage table
        add_cage(child_cageid, cageobj);

        // Copy the 3i handler table from parent to child.
        //
        // This ensures that the child process inherits all syscall
        // interposition and routing behavior, including RawPOSIX's
        // syscall implementation
        threei::copy_handler_table_to_cage(
            UNUSED_ARG,
            child_cageid,
            parent_cageid,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        );
    }

    // Delegate execution back to binary runtime (currently only support Wasmtime,
    // but could be extended to other runtime ie: MPK in the future) via 3i.
    //
    // Call from RawPOSIX (selfcageid = RawPOSIX_CAGEID) into
    // Wasmtime(targetcageid = WASMTIME_CAGEID)
    // to complete the clone/fork operation.
    //
    // Wasmtime will:
    //   - Resolve the correct VMContext
    //   - Complete fork semantics
    //   - Resume execution in parent and child
    threei::make_syscall(
        RAWPOSIX_CAGEID,
        syscall_const::CLONE_SYSCALL as u64,
        UNUSED_NAME,
        WASMTIME_CAGEID,
        clone_arg,
        clone_arg_cageid,
        parent_cageid,
        parent_tid,
        child_cageid,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    )
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
pub extern "C" fn exec_syscall(
    cageid: u64,
    path: u64,
    path_cageid: u64,
    argv: u64,
    argv_cageid: u64,
    envs: u64,
    envs_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "exec_syscall"
        );
    }

    let ret = threei::make_syscall(
        RAWPOSIX_CAGEID,
        syscall_const::EXEC_SYSCALL as u64,
        UNUSED_NAME,
        WASMTIME_CAGEID,
        path,
        cageid, // Pass cageid as the second argument to identify the execing cage in wasmtime
        argv,
        argv_cageid,
        envs,
        envs_cageid,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );

    // Clean up the cage only if exec succeeds.
    // A return value < 0 indicates exec failure.
    //
    // We rely on Asyncify to detect success:
    // if Asyncify begins unwinding, exec has succeeded.
    // By convention, Asyncify unwind returns 0, which we use as the success signal.
    if ret == 0 {
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
        // Do NOT clear epoch_handler or main_threadid here.
        // If exec-ed module crashes, the thread is still running and needs its
        // epoch_handler entry for proper exit tracking.  On success,
        // wasmtime re-instantiates and lind_signal_init (called from
        // lind-multi-process/src/lib.rs during new instance setup) will
        // overwrite the stale entries in epoch_handler and main_threadid.
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/exit.3.html
/// Syscall 60 — exit the calling thread only.
///
/// Used by start_thread (pthread_create.c) when a non-main thread returns
/// from its thread function.  Does NOT initiate cage-wide shutdown — that
/// is exit_group's job (syscall 231).
/// See also: exit_group_syscall (syscall 231).
pub extern "C" fn exit_syscall(
    cageid: u64,
    status_arg: u64,
    status_cageid: u64,
    tid: u64,
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
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "exit_syscall"
        );
    }

    let status = sc_convert_sysarg_to_i32(status_arg, status_cageid, cageid);

    // Thread-only exit: just record status and trigger asyncify unwind.
    // No CAS, no epoch_kill_all — the cage stays alive for other threads.
    cage::cage_record_exit_status(cageid, ExitStatus::Exited(status));

    // Call wasmtime to trigger asyncify unwind for this thread.
    // OnCalledAction handles lind_thread_exit + cage_finalize if last.
    threei::make_syscall(
        RAWPOSIX_CAGEID,
        syscall_const::EXIT_SYSCALL as u64,
        UNUSED_NAME,
        WASMTIME_CAGEID,
        status_arg,
        cageid,
        tid,
        UNUSED_ARG, // is_last_thread: determined dynamically in OnCalledAction
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    )
}

/// Syscall 231 — exit_group: terminate all threads in the cage.
///
/// Called by glibc exit() after running atexit handlers and flushing stdio.
/// Kills all other threads via epoch, then exits the calling thread.
pub extern "C" fn exit_group_syscall(
    cageid: u64,
    status_arg: u64,
    status_cageid: u64,
    tid: u64,
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
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "exit_group_syscall"
        );
    }

    let status = sc_convert_sysarg_to_i32(status_arg, status_cageid, cageid);

    // Only the first thread to win the CAS initiates cage shutdown.
    // Other threads just fall through and will be killed via epoch_kill_all.
    if cage::signal::signal::try_initiate_exit_group(cageid) {
        cage::cage_record_exit_status(cageid, ExitStatus::Exited(status));

        if let Some(c) = cage::get_cage(cageid) {
            c.is_dead.store(true, std::sync::atomic::Ordering::Release);
        }

        // Mark cage as exiting BEFORE epoch_kill_all — killed threads
        // wake up and may make syscalls; EXITING_TABLE must be set so
        // make_syscall returns -ESRCH instead of dispatching.
        //
        // TODO: this cleanup (EXITING_TABLE insert, epoch_kill_all,
        // _rm_grate_from_handler) should be moved into
        // trigger_harsh_cage_exit / harsh_cage_exit so all exit paths
        // go through a single cage teardown sequence.  We currently
        // inline it here because trigger_harsh_cage_exit dispatches
        // harsh_cage_exit through the grate chain, which causes
        // additional syscall traffic during exit and widens race
        // windows with concurrent threads.
        threei::EXITING_TABLE.insert(cageid);

        cage::signal::signal::epoch_kill_all(cageid, tid as i32);
        threei::handler_table::_rm_grate_from_handler(cageid);
    }

    // Call wasmtime to trigger asyncify unwind.
    // OnCalledAction handles lind_thread_exit + cage_finalize if last.
    threei::make_syscall(
        RAWPOSIX_CAGEID,
        60, // reuse exit trampoline in wasmtime
        UNUSED_NAME,
        WASMTIME_CAGEID,
        status_arg,
        cageid,
        tid,
        UNUSED_ARG, // is_last_thread: determined dynamically in OnCalledAction
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    )
}

// Sanitized representation of waitpid's child selector argument
#[derive(Debug, Clone, Copy)]
enum WaitpidChildSelector {
    AnyChild,
    SpecificChild(i32),
}

// Sanitize and interpret the raw child_cageid argument
fn sanitize_waitpid_child_selector(
    child_cageid_arg: u64,
    child_cageid_arg_cageid: u64,
    cageid: u64,
) -> WaitpidChildSelector {
    let raw_child_id = sc_convert_sysarg_to_i32(child_cageid_arg, child_cageid_arg_cageid, cageid);

    // cageid <= 0 means wait for ANY child
    // cageid > 0 means wait for specific child with that cage ID
    if raw_child_id <= 0 {
        WaitpidChildSelector::AnyChild
    } else {
        WaitpidChildSelector::SpecificChild(raw_child_id)
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/waitpid.3p.html
///
/// waitpid() will return the cageid of waited cage, or 0 when WNOHANG is set and there is no cage already exited
/// waitpid_syscall utilizes the zombie list stored in cage struct. When a cage exited, a zombie entry will be inserted
/// into the end of its parent's zombie list. Then when parent wants to wait for any of child, it could just check its
/// zombie list and retrieve the first entry from it (first in, first out).
pub extern "C" fn waitpid_syscall(
    cageid: u64,
    child_cageid_arg: u64,
    child_cageid_arg_cageid: u64,
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
    let child_selector =
        sanitize_waitpid_child_selector(child_cageid_arg, child_cageid_arg_cageid, cageid);

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

    // Now use the sanitized child_selector enum
    match child_selector {
        WaitpidChildSelector::AnyChild => {
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
                    // Check for pending signals after yielding (only if WNOHANG is not set).
                    // Re-acquire the zombie lock first: the child's exit may have both
                    // added a zombie AND sent SIGCHLD, so the zombie could already be
                    // available. Prefer completing the wait over returning EINTR.
                    zombies = cage.zombies.write();
                    if zombies.len() > 0 {
                        continue;
                    }
                    if (options & WNOHANG == 0) && signal_check_trigger(cage.cageid) {
                        return syscall_error(Errno::EINTR, "waitpid", "interrupted by signal");
                    }
                    continue;
                } else {
                    // there are zombies avaliable
                    // let's retrieve the first zombie
                    zombie_opt = Some(zombies.remove(0));
                    break;
                }
            }
        }
        WaitpidChildSelector::SpecificChild(cage_id_to_wait) => {
            // if cageid is specified, then we need to look up the zombie list for the id
            // first let's check if the cageid is in the zombie list
            if let Some(index) = zombies
                .iter()
                .position(|zombie| zombie.cageid == cage_id_to_wait as u64)
            {
                // find the cage in zombie list, remove it from the list and break
                zombie_opt = Some(zombies.remove(index));
            } else {
                // if the cageid is not in the zombie list, then we know either
                // 1. the child is still running, or
                // 2. the cage has exited, but it is not the child of this cage, or
                // 3. the cage does not exist
                // we need to make sure the child is still running, and it is the child of this cage
                let child = get_cage(cage_id_to_wait as u64);
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
                    // Re-acquire the zombie lock before checking signals: the child's
                    // exit may have both added a zombie AND sent SIGCHLD atomically,
                    // so the zombie could already be available. Prefer completing the
                    // wait over returning EINTR.
                    zombies = cage.zombies.write();

                    // let's check if the zombie list contains the cage
                    if let Some(index) = zombies
                        .iter()
                        .position(|zombie| zombie.cageid == cage_id_to_wait as u64)
                    {
                        // find the cage in zombie list, remove it from the list and break
                        zombie_opt = Some(zombies.remove(index));
                        break;
                    }

                    // Check for pending signals after yielding (only if WNOHANG is not set)
                    if (options & WNOHANG == 0) && signal_check_trigger(cage.cageid) {
                        return syscall_error(Errno::EINTR, "waitpid", "interrupted by signal");
                    }

                    continue;
                }
            }
        }
    }

    // reach here means we already found the desired exited child
    let zombie = zombie_opt.unwrap();
    // update the status
    if let Some(status) = status {
        *status = encode_wait_status(zombie.exit_code);
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
pub extern "C" fn getpid_syscall(
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getpgid.2.html
///
/// Returns the process group ID of the process specified by pid.
/// If pid is 0, returns the process group ID of the calling process.
///
/// Lind does not implement process groups. The default RawPOSIX behavior
/// always returns the cage's own cageid. A grate can interpose on this
/// syscall to provide different process group semantics.
///
/// ## Returns
///     - The cageid (as process group ID) on success.
pub extern "C" fn getpgid_syscall(
    cageid: u64,
    pid_arg: u64,
    pid_cageid: u64,
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
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getpgid_syscall"
        );
    }

    // Lind doesn't implement process groups. Return own cageid regardless
    // of the pid argument (matching the behavior of getpid).
    let cage = get_cage(cageid).unwrap();
    cage.cageid as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man3/getppid.3p.html
///
/// See comments of `getpid_syscall` for more details
///
/// ## Returns
/// Get the parent cage ID
pub extern "C" fn getppid_syscall(
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
pub extern "C" fn getgid_syscall(
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
pub extern "C" fn getegid_syscall(
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
pub extern "C" fn getuid_syscall(
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
pub extern "C" fn geteuid_syscall(
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
pub extern "C" fn sigaction_syscall(
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
pub extern "C" fn kill_syscall(
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
pub extern "C" fn sigprocmask_syscall(
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/prlimit.2.html
///
/// Reads or sets resource limits.
/// Each resource has an associated soft and hard limit defined by rlimit struct.
/// soft limit is the value that kernel enforces for the reponse. Hard limit the ceiling for how high the soft limit can be set.
/// An unprevileged process may set only the soft limit and irreversibly lower hard limit.
/// A previleged process may make arbitrary changes to either hard/soft values.
/// ## Returns
/// On success, returns 0. On error, -1 is returned, and errno is set.

pub extern "C" fn prlimit64_syscall(
    cageid: u64,
    arg1: u64, //arg1: pid (0 = current process)
    arg1_cageid: u64,
    arg2: u64, //arg2: which resource( RLIMIT_NOFILE, etc)
    arg2_cageid: u64,
    arg3: u64, //arg3: pointer to new limit (Null for getrlimit)
    arg3_cageid: u64,
    arg4: u64, //arg4: pointer to receive current limit (NULL for setrlimit)
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    //pid has to be zero
    let pid = sc_convert_sysarg_to_i32(arg1, arg1_cageid, cageid);
    if pid != 0 {
        lind_debug_panic(&format!("prlimit64: unsupported pid {}", pid));
        return syscall_error(Errno::ESRCH, "prlimit64", "Only supports pid = 0");
    }

    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "prlimit64_syscall",
        );
    }
    // get resource numeber from arg2
    let resource = sc_convert_sysarg_to_u32(arg2, arg2_cageid, cageid);

    // setrlimit unsupported
    if !sc_convert_arg_nullity(arg3, arg3_cageid, cageid) {
        lind_debug_panic("prlimit64: setrlimit not supported");
        return syscall_error(Errno::EPERM, "prlimit64", "setrlimit not supported");
    }

    // handle getrlimit calls
    // default to 1024.
    if !sc_convert_arg_nullity(arg4, arg4_cageid, cageid) {
        let old_limit = match sc_convert_addr_to_rlimit(arg4, arg4_cageid, cageid) {
            Ok(rlim) => rlim,
            Err(e) => return syscall_error(e, "prlimit64", "bad address"),
        };
        match resource {
            RLIMIT_STACK => {
                old_limit.rlim_cur = 8 * 1024 * 1024;
                old_limit.rlim_max = 8 * 1024 * 1024;
            }
            RLIMIT_NOFILE => {
                old_limit.rlim_cur = 1024;
                old_limit.rlim_max = 1024;
            }
            RLIMIT_DATA | RLIMIT_RSS | RLIMIT_AS => {
                old_limit.rlim_cur = MAX_LINEAR_MEMORY_SIZE as u32;
                old_limit.rlim_max = MAX_LINEAR_MEMORY_SIZE as u32;
            }
            RLIMIT_NPROC => {
                old_limit.rlim_cur = MAX_CAGEID as u32;
                old_limit.rlim_max = MAX_CAGEID as u32;
            }
            RLIMIT_CORE => {
                old_limit.rlim_cur = 0;
                old_limit.rlim_max = 0;
            }
            _ => {
                lind_debug_panic(&format!("prlimit64: unsupported resource {}", resource));
                old_limit.rlim_cur = 0;
                old_limit.rlim_max = 0;
            }
        }
    }

    0 //success
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/sched_yield.2.html
///
/// Causes the calling thread to relinquish the CPU. The thread is moved to the end
/// of the queue for its static priority and a new thread gets to run.
///
/// ## Returns
/// On success, returns 0. On error, -1 is returned, and errno is set.
pub extern "C" fn sched_yield_syscall(
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
pub extern "C" fn setitimer_syscall(
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
