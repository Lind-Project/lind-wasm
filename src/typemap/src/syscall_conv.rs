//! Top level Type Conversion API
//!
//! This file provides the top level type conversion API needed for actual syscall implementation
//! under src/syscalls/
use crate::path_conv::*;
use crate::type_conv::*;
use cage::get_cage;
use cage::memory::mem_helper::*;
use fdtables;
use std::error::Error;
use std::str::Utf8Error;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{MAX_CAGEID, PATH_MAX};

/// Translate a received virtual file descriptor (`virtual_fd`) to real kernel file descriptor.
/// This function is not for security purpose. Always using arg_cageid to translate.
///     - If arg_cageid != cageid: this call is sent by grate. We need to translate according to cage
///     - If arg_cageid == cageid: this call is sent by cage, we can use either one
/// Return: underlying kernel file descriptor
pub fn convert_fd_to_host(virtual_fd: u64, arg_cageid: u64, cageid: u64) -> i32 {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(path_arg_cageid, cageid) {
            return -1;
        }
    }
    // Find corresponding virtual fd instance from `fdtable` subsystem
    let wrappedvfd = fdtables::translate_virtual_fd(arg_cageid, virtual_fd);
    if wrappedvfd.is_err() {
        return -9;
    }
    let vfd = wrappedvfd.unwrap();
    // Actual kernel fd mapped with provided virtual fd
    vfd.underfd as i32
}

/// This function provides two operations: first, it translates path pointer address from WASM environment
/// to kernel system address; then, it adjusts the path from user's perspective to host's perspective,
/// which is adding `LIND_ROOT` before the path arguments. Considering actual syscall implementation
/// logic needs to pass string pointer to underlying rust libc, so this function will return `CString`
/// lways using arg_cageid to translate. (TODO: the logic here might be different according to 3i/grate
/// implementation)
///     - If arg_cageid != cageid: this call is sent by grate. We need to translate according to cage
///     - If arg_cageid == cageid: this call is sent by cage, we can use either one
/// 
/// ##Arguments:
///     - cageid: required to do address translation for path pointer
///     - path_arg: the path pointer with address and contents from user's perspective. Address is
///                 32-bit (because of WASM feature).
///
/// ##Outputs:
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
pub fn sc_convert_path_to_host(path_arg: u64, path_arg_cageid: u64, cageid: u64) -> CString {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(path_arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }
    let cage = get_cage(path_arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, path_arg).unwrap();
    let path = match get_cstr(addr) {
        Ok(path) => path,
        Err(e) => panic!("{:?}", e),
    };
    // We will create a new variable in host process to handle the path value
    let relpath = normpath(convpath(path), path_arg_cageid);
    let relative_path = relpath.to_str().unwrap();

    #[cfg(feature = "secure")]
    {
        let total_length = LIND_ROOT.len() + relative_path.len();

        if total_length >= PATH_MAX {
            panic!("Path exceeds PATH_MAX (4096)");
        }
    }

    // CString will handle the case when string is not terminated by `\0`, but will return error if `\0` is
    // contained within the string.
    let full_path = format!("{}{}", LIND_ROOT, relative_path);
    match CString::new(full_path) {
        Ok(c_path) => c_path,
        Err(_) => panic!("String contains internal null byte"),
    }
}

/// Validate whether two cage ids are in valid range. This is used for security mode in
/// type conversion.
/// 
/// ## Arguments:
/// cageid_1: first cage id
/// cageid_2: second cage id
/// 
/// ## Output:
/// true: both of them are valid
/// false: one of them or neither of them are valid 
pub fn validate_cageid(cageid_1: u64, cageid_2: u64) -> bool {
    if cageid_1 > MAX_CAGEID as u64 || cageid_2 > MAX_CAGEID as u64 {
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
/// ## Arguments:
/// arg: the argument value
/// arg_cageid: the argument's cageid
/// cageid: source cageid (the cage execute this call)
/// 
/// ## Output:
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
/// ## Output:
/// Success: A converted i32
/// Fail: panic
pub fn sc_convert_sysarg_to_i32(arg: u64, arg_cageid: u64, cageid: u64) -> i32 {
    #[cfg(feature = "fast")]
    return arg as i32;

    #[cfg(feature = "secure")]
    return get_i32(arg, arg_cageid, cageid);
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
/// ## Output:
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

/// ## Arguments:
/// arg: the argument value
/// arg_cageid: the argument's cageid
/// cageid: source cageid (the cage execute this call)
/// 
/// ## Output:
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
/// ## Output: 
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
/// ## Output: 
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
/// ## Output: 
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
/// ## Output:
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
/// ## Output:
///     - buf: actual system address, which is the actual position that stores data
pub fn sc_convert_buf(buf_arg: u64, arg_cageid: u64, cageid: u64) -> *const u8 {
    // Get cage reference to translate address
    let cage = get_cage(arg_cageid).unwrap();
    // Convert user buffer address to system address. We don't need to check permission here.
    // Permission check has been handled in 3i
    let buf = translate_vmmap_addr(&cage, buf_arg).unwrap() as *const u8;
    buf
}
