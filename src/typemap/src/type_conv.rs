// //! This file provides essential functions for handling and validating `u64` inputs, converting
// //! them to various system-specific data types needed in system calls.  It includes utilities
// //! for transforming raw pointers to typed structures, such as integer, buffer, and string pointers,
// //! as well as complex structures like polling, signal handling, timing, and socket-related types.
// //! Each function ensures safe and correct usage by performing null checks, boundary validations,
// //! and type casting, returning either a valid reference or an error if data is invalid. This design
// //! promotes secure, reliable access to memory and resources in a low-level systems environment.
// use sysdefs::data::fs_struct;
// use sysdefs::data::net_struct;
use sysdefs::data::fs_struct::PipeArray;

pub fn get_pipearray<'a>(generic_argument: u64) -> Result<&'a mut PipeArray, i32> {
    let pointer = generic_argument as *mut PipeArray;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}
