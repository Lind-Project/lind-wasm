use lind_if::{
    AbiDescriptor, ConstLocalBuffer, Errno, LindAbiConversion, LindArgVec, LindCloneArgs,
    LindError, LindInitArgs, LindIovecAccess, LindMmapArgs, LindMprotectArgs, LindMunmapArgs,
    LindResult, LindRuntime, LindThreadDuplicateArgs, LocalBuffer, ThreadDuplicateMode, UserPtr,
};
use lind_runtimes::{lind_mpk::lind_if::MpkRuntime, lind_wasm::lind_if::WasmRuntime};
use wasmtime_lind_multi_process::{LindCtx, LindHost};

/// Host state for instantiating `WasmRuntime`; tests never reach Wasmtime.
#[derive(Clone)]
struct StubHost;

impl LindHost<StubHost, ()> for StubHost {
    fn get_ctx(&self) -> LindCtx<StubHost, ()> {
        unreachable!()
    }

    fn get_ctx_mut(&mut self) -> &mut LindCtx<StubHost, ()> {
        unreachable!()
    }
}

static WASM: WasmRuntime<StubHost, ()> = WasmRuntime::new();

fn runtimes() -> [&'static dyn LindRuntime; 2] {
    [&WASM, &MpkRuntime]
}

fn assert_unimplemented<T: std::fmt::Debug>(result: LindResult<T>) {
    assert!(
        matches!(&result, Err(LindError::Other(Errno::ENOSYS))),
        "expected ENOSYS, got {result:?}"
    );
}

#[test]
fn unmigrated_lifecycle_and_reservation_operations_fail_explicitly() {
    let init = LindInitArgs {
        argv: LindArgVec::Host(vec![]),
        envp: LindArgVec::Host(vec![]),
    };
    let clone = LindCloneArgs {
        flags: 0,
        stack: None,
        tls: None,
    };
    let mmap = LindMmapArgs {
        address: 0,
        length: 4096,
        prot: 0,
        flags: 0,
        fd: -1,
        offset: 0,
    };

    for runtime in runtimes() {
        assert_unimplemented(runtime.belongs_to_runtime("program"));
        assert_unimplemented(runtime.get_host_address(1, 0x1000));
        assert_unimplemented(runtime.get_cage_abi(1));
        // Only staged init from guest argv/envp is implemented.
        assert_unimplemented(runtime.init_cage(1, "program", &init, false));
        assert_unimplemented(runtime.init_cage(2, "program", &init, true));
        // Thread creation is only implemented for a staged cage's main thread.
        assert_unimplemented(runtime.create_thread(1, 1, &clone));
        assert_unimplemented(runtime.do_mmap(1, &mmap, true));
    }
}

#[test]
fn staging_operations_reject_unstaged_cages() {
    let clone = LindCloneArgs {
        flags: 0,
        stack: None,
        tls: None,
    };
    for runtime in runtimes() {
        assert!(matches!(
            runtime.apply_staging_cage(2, 1),
            Err(LindError::Other(Errno::EINVAL))
        ));
        assert!(matches!(
            runtime.create_thread(1, 2, &clone),
            Err(LindError::Other(Errno::EINVAL))
        ));
    }
}

#[test]
fn duplicate_rejects_unsupported_and_inconsistent_requests() {
    let duplicate = |mode| LindThreadDuplicateArgs {
        mode,
        clone_args: UserPtr {
            address: 0,
            cage_id: 1,
        },
        set_parent_tid: None,
        set_child_tid: None,
        parent_result: 0,
        child_result: 0,
    };
    let einval = |result: LindResult<()>| {
        assert!(
            matches!(result, Err(LindError::Other(Errno::EINVAL))),
            "expected EINVAL, got {result:?}"
        )
    };

    for runtime in runtimes() {
        einval(runtime.duplicate_cage(1, 1));
        assert_unimplemented(runtime.duplicate_thread(
            1,
            1,
            2,
            1,
            &duplicate(ThreadDuplicateMode::RawInterrupted),
        ));
        einval(runtime.duplicate_thread(1, 1, 1, 1, &duplicate(ThreadDuplicateMode::SyscallFork)));
        einval(runtime.duplicate_thread(1, 1, 2, 0, &duplicate(ThreadDuplicateMode::PthreadClone)));
    }
}

#[test]
fn runtime_mappings_apply_host_operations_and_report_errors() {
    for runtime in runtimes() {
        let mut args = LindMmapArgs {
            address: 0,
            length: 4096,
            prot: libc::PROT_READ | libc::PROT_WRITE,
            flags: libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            fd: -1,
            offset: 0,
        };
        let address = runtime.do_mmap(1, &args, false).unwrap();
        unsafe { *(address as *mut u8) = 0xa5 };
        runtime
            .do_mprotect(
                1,
                &LindMprotectArgs {
                    address,
                    length: args.length,
                    prot: libc::PROT_READ,
                },
            )
            .unwrap();
        assert_eq!(unsafe { *(address as *const u8) }, 0xa5);

        assert!(matches!(
            runtime.do_mprotect(
                1,
                &LindMprotectArgs {
                    address: address + 1,
                    length: args.length,
                    prot: libc::PROT_READ,
                }
            ),
            Err(LindError::Other(Errno::EINVAL))
        ));
        assert!(matches!(
            runtime.do_munmap(
                1,
                &LindMunmapArgs {
                    address: address + 1,
                    length: args.length,
                }
            ),
            Err(LindError::Other(Errno::EINVAL))
        ));
        runtime
            .do_munmap(
                1,
                &LindMunmapArgs {
                    address,
                    length: args.length,
                },
            )
            .unwrap();

        args.length = 0;
        assert!(matches!(
            runtime.do_mmap(1, &args, false),
            Err(LindError::Other(Errno::EINVAL))
        ));
    }
}

fn check_unavailable_accessors(
    conversion: &dyn LindAbiConversion<u64>,
    iovec: &dyn LindIovecAccess,
    abi: AbiDescriptor,
) {
    let mut output = [0x5a; 16];
    let local = LocalBuffer {
        address: output.as_mut_ptr(),
        len: output.len(),
    };
    let input = ConstLocalBuffer {
        address: std::ptr::null(),
        len: 8,
    };

    assert_unimplemented(conversion.get_runtime_from_lind(abi, std::ptr::null(), local));
    assert_unimplemented(conversion.get_lind_from_runtime(abi, input, std::ptr::null_mut()));
    assert_unimplemented(iovec.set_iovec_base(abi, local, 0, 0x1000));
    assert_unimplemented(iovec.set_guest_iovec_base(abi, 1, 0x2000, 0, 0x1000));
    assert_eq!(output, [0x5a; 16]);
}

#[test]
fn unmigrated_conversions_reject_untrusted_pointers_without_touching_memory() {
    check_unavailable_accessors(&WASM, &WASM, AbiDescriptor::Wasm32);
    check_unavailable_accessors(&MpkRuntime, &MpkRuntime, AbiDescriptor::LinuxX86_64);
}

#[test]
fn runtime_mmap_uses_host_fd_offset_and_shared_backing() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    let fd = unsafe { libc::memfd_create(c"lind-if-map".as_ptr(), libc::MFD_CLOEXEC) };
    assert!(fd >= 0);
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    assert_eq!(unsafe { libc::ftruncate(fd.as_raw_fd(), 8192) }, 0);
    for runtime in runtimes() {
        let value = 0x42u8;
        assert_eq!(
            unsafe { libc::pwrite(fd.as_raw_fd(), (&value as *const u8).cast(), 1, 4096) },
            1
        );
        let mut args = LindMmapArgs {
            address: 0,
            length: 4096,
            prot: libc::PROT_READ | libc::PROT_WRITE,
            flags: libc::MAP_SHARED,
            fd: fd.as_raw_fd(),
            offset: 4096,
        };
        let address = runtime.do_mmap(1, &args, false).unwrap();
        assert_eq!(unsafe { *(address as *const u8) }, value);
        unsafe { *(address as *mut u8) = 0x7b };
        let mut stored = 0u8;
        assert_eq!(
            unsafe { libc::pread(fd.as_raw_fd(), (&mut stored as *mut u8).cast(), 1, 4096) },
            1
        );
        assert_eq!(stored, 0x7b);
        assert_eq!(
            unsafe { libc::munmap(address as *mut libc::c_void, 4096) },
            0
        );
        args.fd = -1;
        assert!(matches!(
            runtime.do_mmap(1, &args, false),
            Err(LindError::Other(Errno::EBADF))
        ));
    }
}
