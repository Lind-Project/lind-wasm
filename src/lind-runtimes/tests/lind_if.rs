use lind_runtimes::{lind_mpk::lind_if::MpkRuntime, lind_wasm::lind_if::WasmRuntime};
use typemap::lind_if::{
    AbiDescriptor, ConstLocalBuffer, Errno, LindAbiConversion, LindArgVec, LindCloneArgs,
    LindError, LindInitArgs, LindIovecAccess, LindMmapArgs, LindResult, LindRuntime,
    LindThreadDuplicateArgs, LocalBuffer, ThreadDuplicateMode, UserPtr,
};

fn assert_unimplemented<T: std::fmt::Debug>(result: LindResult<T>) {
    assert!(
        matches!(&result, Err(LindError::Other(Errno::ENOSYS))),
        "expected ENOSYS, got {result:?}"
    );
}

#[test]
fn unmigrated_runtimes_cannot_report_successful_lifecycle_or_mapping() {
    let init = LindInitArgs {
        argv: LindArgVec::Host(vec![]),
        envp: LindArgVec::Host(vec![]),
    };
    let clone = LindCloneArgs {
        flags: 0,
        stack: None,
        tls: None,
    };
    let duplicate = LindThreadDuplicateArgs {
        mode: ThreadDuplicateMode::SyscallFork,
        clone_args: UserPtr {
            address: 0,
            cage_id: 1,
        },
        set_parent_tid: Some(0x1000),
        set_child_tid: Some(0x2000),
    };
    let mmap = LindMmapArgs {
        address: 0,
        length: 4096,
        prot: 0,
        flags: 0,
        fd: -1,
        offset: 0,
    };

    for runtime in [&WasmRuntime as &dyn LindRuntime, &MpkRuntime] {
        assert_unimplemented(runtime.belongs_to_runtime("program"));
        assert_unimplemented(runtime.get_host_address(1, 0x1000));
        assert_unimplemented(runtime.get_cage_abi(1));
        assert_unimplemented(runtime.init_cage(1, "program", &init, false));
        assert_unimplemented(runtime.init_cage(2, "program", &init, true));
        assert_unimplemented(runtime.apply_staging_cage(2, 1));
        assert_unimplemented(runtime.duplicate_cage(1, 2));
        assert_unimplemented(runtime.create_thread(1, 1, &clone));
        assert_unimplemented(runtime.duplicate_thread(1, 1, 2, 2, &duplicate));
        assert_unimplemented(runtime.terminate_thread(1, 1));
        assert_unimplemented(runtime.terminate_cage(1));
        assert_unimplemented(runtime.do_mmap(1, &mmap, true));
        assert_unimplemented(runtime.do_mmap(1, &mmap, false));
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
    check_unavailable_accessors(&WasmRuntime, &WasmRuntime, AbiDescriptor::Wasm32);
    check_unavailable_accessors(&MpkRuntime, &MpkRuntime, AbiDescriptor::LinuxX86_64);
}
