//! Wasm implementation of the draft Lind-IF.
//!
//! The existing Wasmtime backend has not yet been migrated to this interface.

use crate::unsupported;
use typemap::lind_if::{
    AbiDescriptor, CageId, ConstLocalBuffer, GuestAddress, HandlerReturn, HostAddress,
    LindAbiConversion, LindCloneArgs, LindHandlerCall, LindInitArgs, LindIovecAccess, LindMmapArgs,
    LindMprotectArgs, LindMunmapArgs, LindResult, LindRuntime, LindSigInfo,
    LindThreadDuplicateArgs, LocalBuffer, ThreadId,
};

#[derive(Debug, Default)]
pub struct WasmRuntime;

impl LindRuntime for WasmRuntime {
    fn belongs_to_runtime(&self, _exe_path: &str) -> LindResult<bool> {
        unsupported()
    }

    fn get_host_address(
        &self,
        _cage_id: CageId,
        _guest_address: GuestAddress,
    ) -> LindResult<HostAddress> {
        unsupported()
    }

    fn get_guest_address(
        &self,
        _cage_id: CageId,
        _host_address: HostAddress,
    ) -> LindResult<GuestAddress> {
        unsupported()
    }

    fn get_cage_abi(&self, _cage_id: CageId) -> LindResult<AbiDescriptor> {
        unsupported()
    }

    fn init_cage(
        &self,
        _cage_id: CageId,
        _exe_path: &str,
        _init_args: &LindInitArgs,
        _is_staging: bool,
    ) -> LindResult<()> {
        unsupported()
    }

    fn apply_staging_cage(&self, _staging_id: CageId, _target_cage_id: CageId) -> LindResult<()> {
        unsupported()
    }

    fn duplicate_cage(&self, _cage_id: CageId, _new_cage_id: CageId) -> LindResult<()> {
        unsupported()
    }

    fn terminate_cage(&self, _cage_id: CageId) -> LindResult<()> {
        unsupported()
    }

    fn create_thread(
        &self,
        _cage_id: CageId,
        _thread_id: ThreadId,
        _args: &LindCloneArgs,
    ) -> LindResult<()> {
        unsupported()
    }

    fn duplicate_thread(
        &self,
        _source_cage_id: CageId,
        _source_thread_id: ThreadId,
        _destination_cage_id: CageId,
        _new_thread_id: ThreadId,
        _args: &LindThreadDuplicateArgs,
    ) -> LindResult<()> {
        unsupported()
    }

    fn terminate_thread(&self, _cage_id: CageId, _thread_id: ThreadId) -> LindResult<()> {
        unsupported()
    }

    fn send_signal(
        &self,
        _cage_id: CageId,
        _thread_id: ThreadId,
        _info: &LindSigInfo,
    ) -> LindResult<()> {
        unsupported()
    }

    fn run_handler(&self, _cage_id: CageId, _call: &LindHandlerCall) -> LindResult<HandlerReturn> {
        unsupported()
    }

    fn do_mmap(
        &self,
        _cage_id: CageId,
        _args: &LindMmapArgs,
        _is_reservation: bool,
    ) -> LindResult<HostAddress> {
        unsupported()
    }

    fn do_mprotect(&self, _cage_id: CageId, _args: &LindMprotectArgs) -> LindResult<()> {
        unsupported()
    }

    fn do_munmap(&self, _cage_id: CageId, _args: &LindMunmapArgs) -> LindResult<()> {
        unsupported()
    }
}

impl<T> LindAbiConversion<T> for WasmRuntime {
    fn get_runtime_from_lind(
        &self,
        _abi: AbiDescriptor,
        _input: *const T,
        _output: LocalBuffer,
    ) -> LindResult<()> {
        unsupported()
    }

    fn get_lind_from_runtime(
        &self,
        _abi: AbiDescriptor,
        _input: ConstLocalBuffer,
        _output: *mut T,
    ) -> LindResult<()> {
        unsupported()
    }
}

impl LindIovecAccess for WasmRuntime {
    fn set_iovec_base(
        &self,
        _abi: AbiDescriptor,
        _iovec: LocalBuffer,
        _index: usize,
        _new_guest_address: GuestAddress,
    ) -> LindResult<()> {
        unsupported()
    }

    fn set_guest_iovec_base(
        &self,
        _abi: AbiDescriptor,
        _owner_cage_id: CageId,
        _iovec_address: GuestAddress,
        _index: usize,
        _new_guest_address: GuestAddress,
    ) -> LindResult<()> {
        unsupported()
    }
}
