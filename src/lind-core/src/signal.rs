//! Signal sending (Lind-IF §6 SendSignal) and the cross-cage ABI accessor wrapper (§3).

use crate::runtime::runtime_of;
use lind_runtime::{
    abi, AbiDescriptor, CageId, GuestAddress, LindError, LindResult, LindSigInfo, MemoryFaultCause,
    MemoryFaultInfo, ThreadId,
};

/// Send `info` to `cage` (`thread == lind_runtime::ANY_THREAD` for process-directed signals).
pub fn send_signal(cage: CageId, thread: ThreadId, info: LindSigInfo) -> LindResult<()> {
    runtime_of(cage)?.send_signal(cage, thread, info)
}

/// `SetGuestIovecBase`: set the base of iovec entry `index` stored in `owner`'s memory.
pub fn set_guest_iovec_base(
    abi_descriptor: AbiDescriptor,
    owner: CageId,
    iovec_address: GuestAddress,
    index: usize,
    new_guest_address: GuestAddress,
) -> LindResult<()> {
    let runtime = runtime_of(owner)?;
    let field = iovec_address
        .checked_add(abi::iovec_base_offset(abi_descriptor, index)?)
        .ok_or(LindError::MemoryFault(MemoryFaultInfo {
            address: iovec_address,
            cause: MemoryFaultCause::Unmapped,
        }))?;
    let bytes = abi::encode_guest_address(abi_descriptor, new_guest_address)?;
    let host = runtime.get_host_address(owner, field)?;
    if cage::check_addr_rw(owner, host, bytes.len()).is_err() {
        return Err(LindError::MemoryFault(MemoryFaultInfo {
            address: field,
            cause: MemoryFaultCause::PermissionDenied,
        }));
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), host as *mut u8, bytes.len());
    }
    Ok(())
}
