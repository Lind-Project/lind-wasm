//! Unified syscall dispatch shims for lifecycle and memory operations.
//!
//! A single 3i handler is registered per syscall.  Each shim resolves the
//! runtime type of the cage that originally issued the syscall, then
//! drives the cage's `lind-runtimes` implementation through `LindRuntime`.
//!
//! Memory requests originate in RawPOSIX after validation and translation.
//! `arg1_cageid` selects the affected cage; arguments contain host addresses
//! and host fds, not guest pointers or virtual fds. Results are i64 addresses
//! or negative errnos.
//!
//! Argument layout when a shim is invoked (matches `threei::RawCallFunc`):
//!
//!   clone  (CLONE3_SYSCALL = 435), dispatched by RawPOSIX `fork_syscall`:
//!     cageid          = WASMTIME_CAGEID (handler cage, not the calling cage)
//!     arg1            = clone_arg (ptr to clone_args struct)
//!     arg1_cageid     = clone_arg_cageid
//!     arg2            = parent_cageid  ← runtime is looked up here
//!     arg2_cageid     = parent_tid
//!     arg3            = child_cageid (0 for a thread clone)
//!     arg4            = clone flags
//!     arg5            = clone_args.parent_tid (owned by clone_arg_cageid)
//!     arg6            = clone_args.child_tid (owned by clone_arg_cageid)
//!
//!   exec   (EXEC_SYSCALL = 59), dispatched by RawPOSIX `exec_syscall`:
//!     cageid          = WASMTIME_CAGEID
//!     arg1            = path
//!     arg1_cageid     = execing_cageid ← runtime is looked up here
//!     arg2            = argv
//!     …
//!
//!   exit   (EXIT_SYSCALL = 60), dispatched by RawPOSIX `exit_syscall`:
//!     cageid          = WASMTIME_CAGEID
//!     arg1            = exit_status
//!     arg1_cageid     = exiting_cageid ← runtime is looked up here
//!     arg2            = tid
//!     …

use sysdefs::constants::lind_platform_const::{
    RAWPOSIX_CAGEID, UNUSED_ARG, UNUSED_ID, WASMTIME_CAGEID,
};
use sysdefs::constants::sys_const::{
    CLONE_CHILD_SETTID, CLONE_PARENT_SETTID, CLONE_THREAD, CLONE_VFORK, CLONE_VM,
};
use sysdefs::constants::syscall_const::{
    CLONE3_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL, MMAP_SYSCALL, MPROTECT_SYSCALL, MUNMAP_SYSCALL,
};
use threei::threei_const;
use lind_if::{
    CageId, Errno, LindArgVec, LindCloneArgs, LindError, LindInitArgs, LindMmapArgs,
    LindMprotectArgs, LindMunmapArgs, LindResult, LindRuntime, LindThreadDuplicateArgs,
    ThreadDuplicateMode, ThreadId, UserPtr,
};
use typemap::datatype_conversion::sc_convert_ptr_to_host;

use lind_runtimes::lifecycle::{begin_clone, begin_thread_exit, take_clone_result};
use lind_runtimes::lind_mpk::lind_if::MpkRuntime;
use lind_runtimes::lind_wasm::lind_if::WasmRuntime;
use wasmtime_lind_multi_process::THREAD_START_ID;

use crate::cli::CliOptions;
use crate::lind_wasmtime::host::HostCtx;

// ── Runtime dispatch ─────────────────────────────────────────────────────────

/// The Wasm runtime, instantiated with lind-boot's Wasmtime host state and
/// launch options.
static WASM_RUNTIME: WasmRuntime<HostCtx, CliOptions> = WasmRuntime::new();

/// The Lind-IF runtime of `cage_id`.
fn lind_runtime(cage_id: u64) -> &'static dyn LindRuntime {
    match threei::get_cage_runtime(cage_id) {
        Some(threei_const::RUNTIME_TYPE_WASMTIME) => &WASM_RUNTIME,
        Some(threei_const::RUNTIME_TYPE_MPK) => &MpkRuntime,
        runtime => panic!("invalid runtime {runtime:?} for cage {cage_id}"),
    }
}

fn lind_errno(error: LindError) -> i32 {
    match error {
        LindError::Other(errno) => -(errno as i32),
        LindError::MemoryFault(_) => -(Errno::EFAULT as i32),
    }
}

// ── Syscall shims ────────────────────────────────────────────────────────────

/// Universal clone handler: Core's fork and thread-clone transactions on top
/// of `LindRuntime::duplicate_cage` and `LindRuntime::duplicate_thread`.
///
/// The child continuation never returns through this handler, except a child
/// of MPK's optional `mpk-process-fork`. So nothing may follow
/// `duplicate_thread` except selecting the result, and Core work must happen
/// before it.
extern "C" fn shim_clone_handler(
    _cageid: u64,
    clone_arg: u64,
    clone_arg_cageid: u64,
    parent_cageid: u64,
    parent_tid: u64,
    child_cageid: u64,
    _arg3_cageid: u64,
    flags: u64,
    _arg4_cageid: u64,
    parent_tid_addr: u64,
    _arg5_cageid: u64,
    child_tid_addr: u64,
    _arg6_cageid: u64,
) -> i32 {
    let runtime = lind_runtime(parent_cageid);
    let source_thread_id = parent_tid as ThreadId;
    let clone_args = UserPtr {
        address: clone_arg,
        cage_id: clone_arg_cageid,
    };
    // Both addresses are owned by `clone_arg_cageid`; that is the parent's
    // memory and, for fork, the same guest address in the child's copy.
    let set_parent_tid = (flags & CLONE_PARENT_SETTID != 0).then_some(parent_tid_addr);
    let set_child_tid = (flags & CLONE_CHILD_SETTID != 0).then_some(child_tid_addr);

    if flags & CLONE_VM == 0 {
        // Fork. RawPOSIX already created the child cage record, fd table, and
        // handler table.
        assert_ne!(child_cageid, 0, "fork without a child cage");

        if clone_arg_cageid != parent_cageid {
            return clone_on_behalf_of_running_cage();
        }

        // CLONE_VFORK without CLONE_VM (glibc's posix_spawn, which lind builds
        // without CLONE_VM): held-parent protocol. Core registers the child
        // here; the runtime holds the parent continuation in
        // `cage::vfork_wait` once the child runs (Wasm: after the child copied
        // the parent; MPK: after the child thread launched), and the child
        // releases it when it execs (`shim_exec_handler`) or is finalized.
        let vfork = flags & CLONE_VFORK != 0;
        if vfork {
            cage::vfork_hold(child_cageid);
        }

        if let Err(error) = runtime.duplicate_cage(parent_cageid, child_cageid) {
            cage::vfork_release(child_cageid);
            // TODO: roll back the child cage record, fd table, and handler
            // table created by RawPOSIX.
            return lind_errno(error);
        }
        begin_clone();
        let duplicate = LindThreadDuplicateArgs {
            mode: ThreadDuplicateMode::SyscallFork,
            clone_args,
            set_parent_tid,
            set_child_tid,
        };
        if let Err(error) = runtime.duplicate_thread(
            parent_cageid,
            source_thread_id,
            child_cageid,
            THREAD_START_ID as ThreadId,
            &duplicate,
        ) {
            // Neither runtime implements terminate_cage yet; Core's rollback
            // of the child cage (see above) is still missing as well.
            cage::vfork_release(child_cageid);
            let _ = runtime.terminate_cage(child_cageid);
            return lind_errno(error);
        }
        // The parent's result is the child PID.
        take_clone_result().unwrap_or(child_cageid as i32)
    } else if flags & CLONE_THREAD != 0 {
        begin_clone();
        let duplicate = LindThreadDuplicateArgs {
            mode: ThreadDuplicateMode::PthreadClone,
            clone_args,
            set_parent_tid,
            set_child_tid,
        };
        // TODO: Core should allocate the thread ID; both runtimes still assign
        // it themselves, so 0 is passed.
        if let Err(error) =
            runtime.duplicate_thread(parent_cageid, source_thread_id, parent_cageid, 0, &duplicate)
        {
            return lind_errno(error);
        }
        // MPK records the TID it assigned. Wasm injects the parent's result
        // on the Asyncify rewind, so this handler's return value is not
        // observed there.
        take_clone_result().unwrap_or(0)
    } else {
        // CLONE_VM without CLONE_THREAD: a new process sharing the parent's
        // memory (e.g. native glibc's posix_spawn, CLONE_VM | CLONE_VFORK).
        // Implementation steps:
        // 1. Have RawPOSIX allocate a child cage as for fork, without copying
        //    the vmmap.
        // 2. Add a Lind-IF operation that creates a cage aliasing the
        //    source's address space instead of duplicate_cage's copy.
        // 3. duplicate_thread into the child with the supplied stack and TLS.
        // 4. For CLONE_VFORK, suspend the parent continuation until the child
        //    execs or exits.
        lind_errno(lind_runtimes::unsupported::<()>().unwrap_err())
    }
}

/// Fork on behalf of another cage that is not blocked in this syscall.
///
/// This is detected only when the clone arguments belong to a cage other than
/// the one being forked: a grate forwarding a cage's own fork passes that
/// cage's arguments through, and the cage is blocked in its syscall. A request
/// made with the target's own arguments from outside its syscall is not yet
/// detected.
///
/// Implementation steps:
/// 1. Select the target thread; the target cage and thread must be preserved
///    through forwarding instead of using the requester's thread.
/// 2. Stop that thread (SIGSTOP-like, thread-only stop extension) and wait
///    until it has halted, remembering whether it was already stopped.
/// 3. duplicate_cage, then duplicate_thread with
///    `ThreadDuplicateMode::RawInterrupted` from the captured state, without
///    injecting fork return values into source or child.
/// 4. Restore the prior stop state of the source (SIGCONT unless it was
///    stopped before) and return the child ID to the requester only.
fn clone_on_behalf_of_running_cage() -> i32 {
    // TODO: roll back the child cage record created by RawPOSIX.
    lind_errno(lind_runtimes::unsupported::<()>().unwrap_err())
}

extern "C" fn shim_exec_handler(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,  // execing_cageid — whose runtime we dispatch on
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
    let _ = (cageid, arg4, arg4_cageid, arg5, arg5_cageid, arg6, arg6_cageid);
    let execing_cageid = arg1_cageid;
    let runtime = lind_runtime(execing_cageid);
    // Release a parent held by CLONE_VFORK. Linux does so only once exec has
    // passed its point of no return; releasing at the start lets the parent
    // run while a failing exec returns its error to the child, which then
    // exits.
    //TODO: give reference for linux behavior, give reference to test that requires this.
    cage::vfork_release(execing_cageid);

    // RawPOSIX passes host addresses (shifted by the owning cage's logical
    // offset in a gs-forked MPK cage).
    let path = match typemap::get_cstr_lossy(sc_convert_ptr_to_host(arg1, arg1_cageid)) {
        Ok(path) => path,
        Err(_) => return -(Errno::EFAULT as i32),
    };
    let init_args = LindInitArgs {
        argv: LindArgVec::Guest(UserPtr {
            address: arg2,
            cage_id: arg2_cageid,
        }),
        envp: LindArgVec::Guest(UserPtr {
            address: arg3,
            cage_id: arg3_cageid,
        }),
    };

    // Lind-IF exec transaction. The image is staged by the execing cage's
    // runtime: switching runtimes on exec is not supported yet.
    // Implementation steps for a runtime switch:
    // 1. Pick the registered runtime whose belongs_to_runtime(path) is true
    //    and stage the image there.
    // 2. After apply_staging_cage's point of no return, have the source
    //    runtime release the old image (terminate_cage restricted to its own
    //    metadata, open in Lind-IF) and switch the cage's runtime type.
    // 3. The target runtime takes over the calling OS thread as the new
    //    image's initial thread.
    let staging_id = staging_cage_id(execing_cageid);
    if let Err(error) = runtime.init_cage(staging_id, &path, &init_args, true) {
        return lind_errno(error);
    }
    let main_thread = LindCloneArgs {
        flags: 0,
        stack: None,
        tls: None,
    };
    if let Err(error) = runtime.create_thread(staging_id, THREAD_START_ID as ThreadId, &main_thread)
    {
        let _ = runtime.terminate_cage(staging_id);
        return lind_errno(error);
    }
    // Consumes the staging cage. Wasm returns 0 once the old image unwinds
    // (RawPOSIX then performs its exec cleanup); MPK only returns on failure.
    // Errors past the point of no return still return to the old image (see
    // the runtimes).
    match runtime.apply_staging_cage(staging_id, execing_cageid) {
        Ok(()) => 0,
        Err(error) => lind_errno(error),
    }
}

/// Staging cage ID for an exec of `target_cage_id`.
///
/// Deviation until Core allocates staging cages: staging IDs are not entered
/// in the cage table and only name the image a runtime staged. They lie above
/// the 32-bit range, so they never collide with cage IDs.
fn staging_cage_id(target_cage_id: CageId) -> CageId {
    (1 << 32) | target_cage_id
}

extern "C" fn shim_exit_handler(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,  // exiting_cageid — whose runtime we dispatch on
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
    let _ = (cageid, arg2_cageid, arg3, arg3_cageid, arg4, arg4_cageid, arg5, arg5_cageid, arg6, arg6_cageid);
    let exiting_cageid = arg1_cageid;
    let tid = arg2;
    let runtime = lind_runtime(exiting_cageid);

    // The status becomes the thread's (and for the last thread the host's)
    // exit value; exit_group recorded the process-level status in Core.
    begin_thread_exit(arg1 as i32);
    // Commits the calling thread to exit and returns; the result is
    // discarded. The runtime retires the thread at its next scheduling point,
    // once this handler returned through 3i (Wasm: after the Asyncify unwind;
    // MPK: on return to the supervisor's syscall entry), and reports it with
    // Core's ThreadExited, which for the last thread calls terminate_cage and
    // finalizes the cage (zombie, SIGCHLD).
    match runtime.terminate_thread(exiting_cageid, tid as ThreadId) {
        Ok(()) => 0,
        Err(error) => lind_errno(error),
    }
}

// ── Memory dispatch ──────────────────────────────────────────────────────────

fn memory_result(result: LindResult<u64>) -> i64 {
    match result {
        Ok(value) => value as i64,
        Err(LindError::Other(errno)) => -(errno as i64),
        Err(LindError::MemoryFault(fault)) => {
            panic!("host mapping operation unexpectedly accessed guest memory: {fault:?}")
        }
    }
}

extern "C" fn shim_mmap_handler(
    _cageid: u64,
    address: u64,
    cage_id: u64,
    length: u64,
    _length_cageid: u64,
    prot: u64,
    _prot_cageid: u64,
    flags: u64,
    _flags_cageid: u64,
    fd: u64,
    _fd_cageid: u64,
    offset: u64,
    _offset_cageid: u64,
) -> i64 {
    memory_result(lind_runtime(cage_id).do_mmap(
        cage_id,
        &LindMmapArgs {
            address,
            length,
            prot: prot as i32,
            flags: flags as i32,
            fd: fd as i32,
            offset: offset as i64,
        },
        false,
    ))
}

extern "C" fn shim_mprotect_handler(
    _cageid: u64,
    address: u64,
    cage_id: u64,
    length: u64,
    _length_cageid: u64,
    prot: u64,
    _prot_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i64 {
    memory_result(
        lind_runtime(cage_id)
            .do_mprotect(
                cage_id,
                &LindMprotectArgs {
                    address,
                    length,
                    prot: prot as i32,
                },
            )
            .map(|()| 0),
    )
}

extern "C" fn shim_munmap_handler(
    _cageid: u64,
    address: u64,
    cage_id: u64,
    length: u64,
    _length_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i64 {
    memory_result(
        lind_runtime(cage_id)
            .do_munmap(cage_id, &LindMunmapArgs { address, length })
            .map(|()| 0),
    )
}

fn register_memory_entries() {
    for (syscall, handler) in [
        (MMAP_SYSCALL, shim_mmap_handler as threei::RawCallFunc),
        (
            MPROTECT_SYSCALL,
            shim_mprotect_handler as threei::RawCallFunc,
        ),
        (MUNMAP_SYSCALL, shim_munmap_handler as threei::RawCallFunc),
    ] {
        let result = threei::register_handler(
            UNUSED_ID,
            WASMTIME_CAGEID,
            RAWPOSIX_CAGEID,
            syscall as u64,
            threei_const::RUNTIME_TYPE_WASMTIME,
            WASMTIME_CAGEID,
            handler as *const () as u64,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        );
        assert_eq!(
            result, 0,
            "registering runtime memory syscall {syscall} failed"
        );
    }
}

#[cfg(test)]
mod memory_dispatch_tests {
    use super::*;
    use cage::{VmmapBitWidth, VmmapOps};
    use lind_runtimes::lind_mpk::pkeys;
    use lind_runtimes::lind_mpk::CAGE_RESERVATION_SIZE;
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;
    use sysdefs::constants::lind_platform_const::INIT_CAGEID;

    // RawPOSIX global state (INIT_CAGEID, fd tables) can only be initialized
    // once per process, so the tests share it and run serially.
    static CAGE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn start_rawposix() -> std::sync::MutexGuard<'static, ()> {
        static START: std::sync::Once = std::sync::Once::new();
        let guard = CAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        START.call_once(|| {
            // Also installs the no-op SIGUSR2 handler used to interrupt threads.
            rawposix::init::rawposix_start(0);
            register_syscall_entries();
        });
        guard
    }

    fn mapping_key(addr: u64) -> u32 {
        let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
        let mut found = false;
        for line in smaps.lines() {
            if let Some((start, end)) = line.split_whitespace().next().and_then(|s| s.split_once('-')) {
                if let (Ok(start), Ok(end)) =
                    (u64::from_str_radix(start, 16), u64::from_str_radix(end, 16))
                {
                    found = (start..end).contains(&addr);
                }
            }
            if found {
                if let Some(key) = line.strip_prefix("ProtectionKey:") {
                    return key.trim().parse().unwrap();
                }
            }
        }
        panic!("no protection key found for mapping {addr:#x}");
    }

    fn rawposix_call(handler: threei::RawCallFunc, args: [u64; 6]) -> i64 {
        handler(
            INIT_CAGEID,
            args[0],
            INIT_CAGEID,
            args[1],
            INIT_CAGEID,
            args[2],
            INIT_CAGEID,
            args[3],
            INIT_CAGEID,
            args[4],
            INIT_CAGEID,
            args[5],
            INIT_CAGEID,
        )
    }

    #[test]
    fn rawposix_dispatches_memory_operations_to_both_runtimes() {
        let _guard = start_rawposix();
        let mpk_available = pkeys::init().is_ok();
        if !mpk_available {
            eprintln!("skipping protection-key checks: MPK unavailable");
        }
        let cage = cage::get_cage(INIT_CAGEID).unwrap();
        let page = 4096usize;
        let prot = libc::PROT_READ | libc::PROT_WRITE;

        for runtime in [
            threei_const::RUNTIME_TYPE_WASMTIME,
            threei_const::RUNTIME_TYPE_MPK,
        ] {
            threei::set_cage_runtime(INIT_CAGEID, runtime);
            cage.runtime_type.store(runtime, Ordering::Release);
            let reservation = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    CAGE_RESERVATION_SIZE,
                    libc::PROT_NONE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
                    -1,
                    0,
                )
            };
            assert_ne!(reservation, libc::MAP_FAILED);
            let host = reservation as u64 + page as u64;
            let guest = if runtime == threei_const::RUNTIME_TYPE_WASMTIME {
                page as u64
            } else {
                host
            };
            {
                let mut vmmap = cage.vmmap.write();
                vmmap.clear();
                vmmap.set_base_address(reservation as usize);
                vmmap.user_addr_bit_width = if runtime == threei_const::RUNTIME_TYPE_WASMTIME {
                    VmmapBitWidth::Vmmap32Bit
                } else {
                    VmmapBitWidth::Vmmap64Bit
                };
            }
            // A key mismatch means MPK dispatch bypassed the pkey-preserving mapping.
            let key = (mpk_available && runtime == threei_const::RUNTIME_TYPE_MPK)
                .then(|| pkeys::assign_reservation(reservation as usize).unwrap());
            assert_eq!(
                rawposix_call(
                    rawposix::fs_calls::mmap_syscall,
                    [
                        guest,
                        page as u64,
                        prot as u64,
                        (libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED) as u64,
                        u64::MAX,
                        0,
                    ]
                ),
                guest as i64
            );
            if let Some(key) = key {
                assert_eq!(mapping_key(host), key);
            }
            unsafe { *(host as *mut u8) = 0xa5 };
            assert_eq!(
                rawposix_call(
                    rawposix::fs_calls::mprotect_syscall,
                    [host, page as u64, libc::PROT_READ as u64, 0, 0, 0,]
                ),
                0
            );
            {
                let vmmap = cage.vmmap.read();
                let page = vmmap.sys_to_page_num(host as usize);
                assert_eq!(vmmap.find_page(page).unwrap().prot, libc::PROT_READ);
            }
            assert_eq!(unsafe { *(host as *const u8) }, 0xa5);

            assert_eq!(
                rawposix_call(
                    rawposix::fs_calls::mprotect_syscall,
                    [host + 1, page as u64, libc::PROT_READ as u64, 0, 0, 0,]
                ),
                -(libc::EINVAL as i64)
            );
            assert_eq!(
                rawposix_call(
                    rawposix::fs_calls::munmap_syscall,
                    [host, page as u64, 0, 0, 0, 0,]
                ),
                0
            );
            {
                let vmmap = cage.vmmap.read();
                let page = vmmap.sys_to_page_num(host as usize);
                assert!(vmmap.find_page(page).is_none());
            }
            if let Some(key) = key {
                assert_eq!(mapping_key(host), key);
            }

            // mmap_inner errors must survive 3i, independent of host TLS errno.
            unsafe { *libc::__errno_location() = libc::EIO };
            let result = rawposix::fs_calls::mmap_inner(
                INIT_CAGEID,
                std::ptr::null_mut(),
                0,
                prot,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            assert_eq!(result as i64, -(libc::EINVAL as i64));

            assert_eq!(
                unsafe { libc::mprotect(host as *mut libc::c_void, page, prot) },
                0
            );
            assert_eq!(unsafe { *(host as *const u8) }, 0);
            assert_eq!(unsafe { libc::munmap(reservation, CAGE_RESERVATION_SIZE) }, 0);
            if key.is_some() {
                pkeys::release_reservation(reservation as usize);
            }
            threei::remove_cage_runtime(INIT_CAGEID);
        }
    }

    #[test]
    fn core_queues_signals_and_notifies_the_cage_runtime() {
        let _guard = start_rawposix();
        let cage = cage::get_cage(INIT_CAGEID).unwrap();
        let epoch = Box::leak(Box::new(0u64)) as *mut u64;
        cage::lind_signal_init(INIT_CAGEID, epoch, 1, true);
        let read_epoch = || unsafe { std::ptr::read_volatile(epoch) };

        for runtime in [
            threei_const::RUNTIME_TYPE_WASMTIME,
            threei_const::RUNTIME_TYPE_MPK,
        ] {
            unsafe { std::ptr::write_volatile(epoch, 0) };
            cage.runtime_type.store(runtime, Ordering::Release);
            assert!(cage::lind_send_signal(INIT_CAGEID, libc::SIGUSR1));
            assert_eq!(read_epoch(), 0xc0ffee, "runtime {runtime} was not notified");
            let (signo, _, _) = cage::lind_get_first_signal(INIT_CAGEID).unwrap();
            assert_eq!(signo, libc::SIGUSR1);

            // A blocked signal is queued and notified, but the runtime must
            // leave the thread alone (a raised epoch would make blocking
            // calls fail with EINTR) until it is unblocked.
            unsafe { std::ptr::write_volatile(epoch, 0) };
            let mask = cage::convert_signal_mask(libc::SIGUSR1);
            cage.sigset.store(mask, Ordering::Relaxed);
            assert!(cage::lind_send_signal(INIT_CAGEID, libc::SIGUSR1));
            assert_eq!(read_epoch(), 0, "runtime {runtime} woke for a blocked signal");
            assert!(cage::lind_get_first_signal(INIT_CAGEID).is_none());
            assert!(!cage.pending_signals.read().is_empty());

            cage.sigset.store(0, Ordering::Relaxed);
            cage::lind_notify_signal(INIT_CAGEID);
            assert_eq!(read_epoch(), 0xc0ffee, "runtime {runtime} missed the unblock");
            let (signo, _, _) = cage::lind_get_first_signal(INIT_CAGEID).unwrap();
            assert_eq!(signo, libc::SIGUSR1);
            assert!(cage.pending_signals.read().is_empty());
        }

        // Without a registered runtime the signal is still queued by Core.
        unsafe { std::ptr::write_volatile(epoch, 0) };
        cage.runtime_type.store(u64::MAX, Ordering::Release);
        assert!(cage::lind_send_signal(INIT_CAGEID, libc::SIGUSR1));
        assert_eq!(read_epoch(), 0);
        let (signo, _, _) = cage::lind_get_first_signal(INIT_CAGEID).unwrap();
        assert_eq!(signo, libc::SIGUSR1);
    }
}

/// Register the Lind-IF runtimes with Core so it can notify them, e.g. of
/// pending signals, and Core's services with the runtimes.
fn register_lind_runtimes() {
    lind_if::register_core(&cage::CORE);
    cage::register_runtime(threei_const::RUNTIME_TYPE_WASMTIME, &WASM_RUNTIME);
    cage::register_runtime(threei_const::RUNTIME_TYPE_MPK, &MpkRuntime);
}

/// Register the Lind-IF runtimes with Core and the lifecycle and memory shims
/// with 3i, after RawPOSIX initialization and before any guest cage is created.
pub fn register_syscall_entries() {
    register_lind_runtimes();
    register_memory_entries();
    let clone_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,
        RAWPOSIX_CAGEID,
        CLONE3_SYSCALL as u64,
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime_id: required by the API, unused by handler_table
        WASMTIME_CAGEID,
        shim_clone_handler as *const () as u64,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID,
    );

    let exec_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,
        RAWPOSIX_CAGEID,
        EXEC_SYSCALL as u64,
        threei_const::RUNTIME_TYPE_WASMTIME,
        WASMTIME_CAGEID,
        shim_exec_handler as *const () as u64,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID,
    );

    let exit_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,
        RAWPOSIX_CAGEID,
        EXIT_SYSCALL as u64,
        threei_const::RUNTIME_TYPE_WASMTIME,
        WASMTIME_CAGEID,
        shim_exit_handler as *const () as u64,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG,
        UNUSED_ID,
    );

    assert!(
        clone_ret == 0 && exec_ret == 0 && exit_ret == 0,
        "[lind-boot] register_syscall_entries failed: clone={} exec={} exit={}",
        clone_ret, exec_ret, exit_ret
    );
}
