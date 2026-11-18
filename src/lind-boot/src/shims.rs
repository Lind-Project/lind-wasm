//! Unified syscall dispatch shims for lifecycle and memory operations.
//!
//! A single 3i handler is registered per syscall.  Each shim resolves the
//! runtime type of the cage that originally issued the syscall, then
//! delegates to the appropriate runtime implementation via `SyscallRuntime`
//! (lifecycle) or `LindRuntime` (memory).
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
//!     arg3            = child_cageid
//!     …
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
use sysdefs::constants::syscall_const::{
    CLONE3_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL, MMAP_SYSCALL, MPROTECT_SYSCALL, MUNMAP_SYSCALL,
};
use threei::threei_const;
use lind_if::{
    LindError, LindMmapArgs, LindMprotectArgs, LindMunmapArgs, LindResult, LindRuntime,
};

use crate::lind_mpk::execute::MpkRuntime;
use crate::lind_wasmtime::execute::WasmtimeRuntime;

// ── Runtime trait ────────────────────────────────────────────────────────────

/// Trait for runtime-specific syscall handlers.
///
/// Each runtime (Wasmtime, MPK) implements this trait to provide its own
/// handling logic for clone, exec, and exit syscalls. The trait uses the
/// standard 3i calling convention: one cageid + six (arg, arg_cageid) pairs.
pub trait SyscallRuntime {
    /// Handle clone/fork syscall.
    fn handle_clone(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32;

    /// Handle exec syscall.
    fn handle_exec(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32;

    /// Handle exit syscall.
    fn handle_exit(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32;
}

// ── Runtime dispatch ─────────────────────────────────────────────────────────

/// Resolve a runtime type constant to its corresponding trait implementation.
fn get_runtime_handler(runtime_type: u64) -> &'static dyn SyscallRuntime {
    match runtime_type {
        threei_const::RUNTIME_TYPE_WASMTIME => &WasmtimeRuntime,
        threei_const::RUNTIME_TYPE_MPK => &MpkRuntime,
        _ => panic!("get_runtime_handler: unknown runtime_type={}", runtime_type),
    }
}

// ── Syscall shims ────────────────────────────────────────────────────────────

extern "C" fn shim_clone_handler(
    cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,         // parent_cageid — whose runtime we dispatch on
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
    let parent_cageid = arg2;
    let runtime_type = threei::get_cage_runtime(parent_cageid)
        .unwrap_or_else(|| {
            panic!(
                "shim_clone_handler: no runtime found for parent_cageid={}",
                parent_cageid
            )
        });

    let runtime = get_runtime_handler(runtime_type);
    runtime.handle_clone(
        cageid,
        arg1, arg1_cageid,
        arg2, arg2_cageid,
        arg3, arg3_cageid,
        arg4, arg4_cageid,
        arg5, arg5_cageid,
        arg6, arg6_cageid,
    )
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
    let execing_cageid = arg1_cageid;
    let runtime_type = threei::get_cage_runtime(execing_cageid)
        .unwrap_or_else(|| {
            panic!(
                "shim_exec_handler: no runtime found for execing_cageid={}",
                execing_cageid
            )
        });

    let runtime = get_runtime_handler(runtime_type);
    runtime.handle_exec(
        cageid,
        arg1, arg1_cageid,
        arg2, arg2_cageid,
        arg3, arg3_cageid,
        arg4, arg4_cageid,
        arg5, arg5_cageid,
        arg6, arg6_cageid,
    )
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
    let exiting_cageid = arg1_cageid;
    let runtime_type = threei::get_cage_runtime(exiting_cageid)
        .unwrap_or_else(|| {
            panic!(
                "shim_exit_handler: no runtime found for exiting_cageid={}",
                exiting_cageid
            )
        });

    let runtime = get_runtime_handler(runtime_type);
    runtime.handle_exit(
        cageid,
        arg1, arg1_cageid,
        arg2, arg2_cageid,
        arg3, arg3_cageid,
        arg4, arg4_cageid,
        arg5, arg5_cageid,
        arg6, arg6_cageid,
    )
}

// ── Memory dispatch ──────────────────────────────────────────────────────────

fn memory_runtime(cage_id: u64) -> &'static dyn LindRuntime {
    match threei::get_cage_runtime(cage_id) {
        Some(threei_const::RUNTIME_TYPE_WASMTIME) => {
            &lind_runtimes::lind_wasm::lind_if::WasmRuntime
        }
        Some(threei_const::RUNTIME_TYPE_MPK) => &lind_runtimes::lind_mpk::lind_if::MpkRuntime,
        runtime => panic!("memory operation: invalid runtime {runtime:?} for cage {cage_id}"),
    }
}

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
    memory_result(memory_runtime(cage_id).do_mmap(
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
        memory_runtime(cage_id)
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
        memory_runtime(cage_id)
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
    use crate::lind_mpk::pkeys;
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
/// pending signals.
fn register_lind_runtimes() {
    cage::register_runtime(
        threei_const::RUNTIME_TYPE_WASMTIME,
        &lind_runtimes::lind_wasm::lind_if::WasmRuntime,
    );
    cage::register_runtime(
        threei_const::RUNTIME_TYPE_MPK,
        &lind_runtimes::lind_mpk::lind_if::MpkRuntime,
    );
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
