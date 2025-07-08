//! Basic primitive type conversion API
//!
//! This file defines conversion helpers for basic primitive types (e.g., `i32`, `u32`, `i64`).
//! These functions are used during syscall argument decoding and type-safe interpretation
//! within the RawPOSIX syscall layer (`src/syscalls/`).
use crate::fs_type_conversion::*;
use crate::network_type_conversion::*;
use cage::get_cage;
use cage::memory::mem_helper::*;
use fdtables;
use std::error::Error;
use std::str::Utf8Error;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{MAX_CAGEID, PATH_MAX};

/// This function provides two operations: first, it translates path pointer address from WASM environment
/// to kernel system address; then, it adjusts the path from user's perspective to host's perspective,
/// which is adding `LIND_ROOT` before the path arguments. Considering actual syscall implementation
/// logic needs to pass string pointer to underlying rust libc, so this function will return `CString`
/// lways using arg_cageid to translate. (TODO: the logic here might be different according to 3i/grate
/// implementation)
///     - If arg_cageid != cageid: this call is sent by grate. We need to translate according to cage
///     - If arg_cageid == cageid: this call is sent by cage, we can use either one
///
/// ## Arguments:
///     - cageid: required to do address translation for path pointer
///     - path_arg: the path pointer with address and contents from user's perspective. Address is
///                 32-bit (because of WASM feature).
///
/// ## Returns:
///     - c_path: a `CString` variable stores the path from host's perspective
///     - will return error if total length exceed the MAX_PATH (which is 4096). We use `Box<dyn Error>` here to
///      let upper functions do error handling. (ie: we want to )
pub unsafe fn charstar_to_ruststr<'a>(cstr: *const i8) -> Result<&'a str, Utf8Error> {
    std::ffi::CStr::from_ptr(cstr as *const _).to_str() //returns a result to be unwrapped later
}

pub fn get_cstr<'a>(arg: u64) -> Result<&'a str, i32> {
    let ptr = arg as *const i8;
    if !ptr.is_null() {
        if let Ok(data) = unsafe { charstar_to_ruststr(ptr) } {
            return Ok(data);
        } else {
            return Err(-1);
        }
    }

    return Err(-1);
}

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

/// `sc_convert_sysarg_to_i32` is the type conversion function used to convert the
/// argument's type from u64 to i32. When in `secure` mode, extra checks will be
/// performed through `get_i32()` function. (for example validating if all upper-bit
/// are 0; if cage ids are in valid range). The security mode can be enabled through
/// compilation flag of this library. Those calls will panic when failed the check
/// for security concerns  
///
/// `get_i32()`
///
/// ## Arguments:
/// arg: the argument value
/// arg_cageid: the argument's cageid
/// cageid: source cageid (the cage execute this call)
///
/// ## Returns:
/// Success: A converted i32
/// Fail: panic
pub fn get_i32(arg: u64, arg_cageid: u64, cageid: u64) -> i32 {
    if !validate_cageid(arg_cageid, cageid) {
        panic!("Invalide Cage ID");
    }

    if (arg & 0xFFFFFFFF_00000000) != 1 {
        return (arg & 0xFFFFFFFF) as i32;
    }

    panic!("Invalide argument");
}

/// ## Arguments:
/// arg: the argument value
/// arg_cageid: the argument's cageid
/// cageid: source cageid (the cage execute this call)
///
/// ## Returns:
/// Success: A converted i32
/// Fail: panic
pub fn sc_convert_sysarg_to_i32(arg: u64, arg_cageid: u64, cageid: u64) -> i32 {
    #[cfg(feature = "fast")]
    return arg as i32;

    #[cfg(feature = "secure")]
    return get_i32(arg, arg_cageid, cageid);
}

/// This function translates 64 bits uadd from the WASM context
/// into the corresponding host address value. Unlike the previous two functions, it returns
/// the translated address as a raw `u64` rather than a pointer.
///
/// Input:
///     - uaddr_arg: the original 64-bit address from the WASM space
///     - uaddr_arg_cageid: the cage ID that owns the address
///     - cageid: the currently executing cage ID
///
/// Output:
///     - Returns the translated 64-bit address in host space as a u64.
pub fn sc_convert_uaddr_to_host(uaddr_arg: u64, uaddr_arg_cageid: u64, cageid: u64) -> u64{
    let cage = get_cage(uaddr_arg_cageid).unwrap();
    let uaddr = translate_vmmap_addr(&cage, uaddr_arg).unwrap();
    return uaddr;
}

/// `sc_convert_sysarg_to_u32` is the type conversion function used to convert the
/// argument's type from u64 to u32. When in `secure` mode, extra checks will be
/// performed through `get_u32()` function. (for example validating if all upper-bit
/// are 0; if cage ids are in valid range). The security mode can be enabled through
/// compilation flag of this library. Those calls will panic when failed the check
/// for security concerns  
///
/// `get_u32()`
/// ## Arguments:
/// arg: the argument value
/// arg_cageid: the argument's cageid
/// cageid: source cageid (the cage execute this call)
///
/// ## Returns:
/// Success: A converted u32
/// Fail: panic
pub fn get_u32(arg: u64, arg_cageid: u64, cageid: u64) -> u32 {
    if !validate_cageid(arg_cageid, cageid) {
        panic!("Invalide Cage ID");
    }

    if (arg & 0xFFFFFFFF_00000000) != 1 {
        return (arg & 0xFFFFFFFF) as u32;
    }

    panic!("Invalide argument");
}

pub fn sc_convert_sysarg_to_i32_ref<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> &'a mut i32 {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let cage = get_cage(arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    return unsafe { &mut *((addr) as *mut i32) };
}

/// ## Arguments:
/// arg: the argument value
/// arg_cageid: the argument's cageid
/// cageid: source cageid (the cage execute this call)
///
/// ## Returns:
/// Success: A converted u32
/// Fail: panic
pub fn sc_convert_sysarg_to_u32(arg: u64, arg_cageid: u64, cageid: u64) -> u32 {
    #[cfg(feature = "fast")]
    return arg as u32;

    #[cfg(feature = "secure")]
    return get_u32(arg);
}

/// `sc_convert_sysarg_to_isize` is the type conversion function used to convert the
/// argument's type from u64 to isize. When in `secure` mode, extra checks will be
/// performed through `validate_cageid()` function (whether cage ids are in valid
/// range). The security mode can be enabled through compilation flag of this library.
/// Those calls will panic when failed the check for security concerns  
///
/// ## Arguments:
/// arg: argument value
/// arg_cageid: argument's cageid
/// cageid: source cageid (the cage executes the call)
///
/// ## Returns:
/// Success: A converted isize
/// Fail: panic
pub fn sc_convert_sysarg_to_isize(arg: u64, arg_cageid: u64, cageid: u64) -> isize {
    #[cfg(feature = "fast")]
    return arg as isize;

    #[cfg(feature = "secure")]
    if !validate_cageid(arg_cageid, cageid) {
        panic!("Invalide Cage ID");
    }
}

/// `sc_convert_sysarg_to_usize` is the type conversion function used to convert the
/// argument's type from u64 to usize. When in `secure` mode, extra checks will be
/// performed through `validate_cageid()` function (whether cage ids are in valid
/// range). The security mode can be enabled through compilation flag of this library.
/// Those calls will panic when failed the check for security concerns  
///
/// ## Arguments:
/// arg: argument value
/// arg_cageid: argument's cageid
/// cageid: source cageid (the cage executes the call)
///
/// ## Returns:
/// Success: A converted usize
/// Fail: panic
pub fn sc_convert_sysarg_to_usize(arg: u64, arg_cageid: u64, cageid: u64) -> usize {
    #[cfg(feature = "fast")]
    return arg as usize;

    #[cfg(feature = "secure")]
    if !validate_cageid(arg_cageid, cageid) {
        panic!("Invalide Cage ID");
    }
}

/// `sc_convert_sysarg_to_i64` is the type conversion function used to convert the
/// argument's type from u64 to i64. When in `secure` mode, extra checks will be
/// performed through `validate_cageid()` function (whether cage ids are in valid
/// range). The security mode can be enabled through compilation flag of this library.
/// Those calls will panic when failed the check for security concerns  
///
/// ## Arguments:
/// arg: argument value
/// arg_cageid: argument's cageid
/// cageid: source cageid (the cage executes the call)
///
/// ## Returns:
/// Success: A converted i64
/// Fail: panic
pub fn sc_convert_sysarg_to_i64(arg: u64, arg_cageid: u64, cageid: u64) -> i64 {
    #[cfg(feature = "fast")]
    return arg as i64;

    #[cfg(feature = "secure")]
    if !validate_cageid(arg_cageid, cageid) {
        panic!("Invalide Cage ID");
    }
}

/// `sc_unusedarg()` is the security check function used to validate all unused args. This
/// will return true in default mode, and check if `arg` with `arg_cageid` are all null in
/// `secure` mode.
///
/// ## Arguments:
/// arg: argument value
/// arg_cageid: argument's cageid
///
/// ## Returns:
/// Always true in default mode.
/// In secure mode:
/// Return true if all null, false otherwise.
pub fn sc_unusedarg(arg: u64, arg_cageid: u64) -> bool {
    #[cfg(feature = "fast")]
    return true;

    #[cfg(feature = "secure")]
    return !(arg | arg_cageid);
}

/// This function translates the buffer pointer from user buffer address to system address, because we are
/// transferring between 32-bit WASM environment to 64-bit kernel
///
/// ## Arguments:
///     - cageid: required to do address translation for buf pointer
///     - buf_arg: the buf pointer address, which is 32-bit because of WASM feature
///
/// ## Returns:
///     - buf: actual system address, which is the actual position that stores data
pub fn sc_convert_buf(buf_arg: u64, arg_cageid: u64, cageid: u64) -> *const u8 {
    // Get cage reference to translate address
    let cage = get_cage(arg_cageid).unwrap();
    // Convert user buffer address to system address. We don't need to check permission here.
    // Permission check has been handled in 3i
    let buf = translate_vmmap_addr(&cage, buf_arg).unwrap() as *const u8;
    buf
}
