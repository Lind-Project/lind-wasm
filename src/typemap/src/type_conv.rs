//! Complex data structure type conversion API
//!
//! This file provides essential functions for handling and validating `u64` inputs, converting
//! them to various system-specific data types needed in system calls.  It includes utilities
//! for transforming raw pointers to typed structures, such as complex structures like polling,
//! signal handling, timing, and socket-related types.
use crate::syscall_conv::validate_cageid;
use sysdefs::data::fs_struct::PipeArray;

/// Convert a raw argument pointer into a mutable reference to a `PipeArray`
///
/// This function interprets the given `u64` value as a raw pointer to a `PipeArray`,
/// and attempts to return a mutable reference to it. It is typically used in
/// syscall argument decoding, where the raw argument comes from Wasm and needs to be
/// reinterpreted into a Rust type.
///
/// ## Arguments:
/// arg: a pointer to `PipeArray`
///
/// ## Returns:
/// `Ok(&mut PipeArray)` if the pointer is non-null and safely transmutable
/// `Err(i32)` (with `EFAULT`) if the pointer is null, indicating an invalid memory reference
pub fn get_pipearray<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> Result<&'a mut PipeArray, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }
    let pointer = arg as *mut PipeArray;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "typemap",
        "input data not valid",
    ));
}
