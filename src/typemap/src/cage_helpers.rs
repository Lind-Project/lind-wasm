//! Cage helper utilities for typemap library.
//!
//! This module provides helper functions to support type conversion
//! and virtual-to-kernel file descriptor translation in the context of
//! cage isolation.
use fdtables;
use sysdefs::constants::lind_const::MAX_CAGEID;
use sysdefs::constants::err_const::Errno;
/// Validate whether two cage ids are in valid range. This is used for security mode in
/// type conversion.
///
/// ## Arguments:
/// cageid_1: first cage id
/// cageid_2: second cage id
///
/// ## Returns:
/// true: both of them are valid
/// false: one of them or neither of them are valid
pub fn validate_cageid(cageid_1: u64, cageid_2: u64) -> bool {
    if cageid_1 > MAX_CAGEID as u64 || cageid_2 > MAX_CAGEID as u64 || cageid_1 < 0 || cageid_2 < 0
    {
        return false;
    }
    true
}

/// Translate a received virtual file descriptor (`virtual_fd`) to real kernel file descriptor.
///
/// This function is used to translate a per-cage virtual file descriptor into the actual
/// kernel-level file descriptor managed by the `fdtables`.
///
/// Optionally, when the `secure` feature is enabled, this function will verify that the
/// `arg_cageid` (the cage that owns the fd being translated) is allowed to interact with the
/// caller's `cageid` (the requesting cage).
///
/// ## Arguments:
/// virtual_fd: The virutal file descriptor
/// arg_cageid: The cage that owns the virtual fd
/// cageid: The cage ID of current caller (only used when `secure` mode is enabled)
///
/// ## Returns:
/// underlying kernel file descriptor
pub fn convert_fd_to_host(virtual_fd: u64, arg_cageid: u64, cageid: u64) -> i32 {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -EINVAL;
        }
    }
    // Find corresponding virtual fd instance from `fdtable` subsystem
    let wrappedvfd = fdtables::translate_virtual_fd(arg_cageid, virtual_fd);
    if wrappedvfd.is_err() {
        return -(Errno::EBADF as i32);
    }
    let vfd = wrappedvfd.unwrap();
    // Actual kernel fd mapped with provided virtual fd
    vfd.underfd as i32
}
