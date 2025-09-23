//! Basic primitive type conversion API
//!
//! This file defines conversion helpers for basic primitive types (e.g., `i32`, `u32`, `i64`).
//! These functions are used during syscall argument decoding and type-safe interpretation
//! within the RawPOSIX syscall layer (`src/syscalls/`).
//! Function naming convention:
//! - All functions starting with `sc_` are **public APIs** exposed to other libraries. Example: `sc_convert_sysarg_to_i32`.
//! - All other functions are **internal helpers** (inner functions) used only inside this library.

pub use libc::*;
pub use std::time::Duration
use cage::memory::mem_helper::*;
use cage::{get_cage, memory::memory::translate_vmmap_addr};
use fdtables;
use std::error::Error;
use std::str::Utf8Error;
use sysdefs::constants::err_const::{syscall_error, Errno};
use std::ptr;
use sysdefs::data::fs_struct::PipeArray;
use sysdefs::constants::fs_const::LIND_ROOT;
use sysdefs::data::fs_struct::{SigactionStruct, SigsetType, ITimerVal};
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, UNUSED_NAME, MAX_CAGEID, PATH_MAX};

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
#[inline]
fn is_unused(val: u64, placeholder: u64) -> bool {
    val == 0 || val == placeholder
}

pub fn sc_unusedarg(arg: u64, arg_cageid: u64) -> bool {
    #[cfg(feature = "fast")]
    return true;

    #[cfg(feature = "secure")]
    return is_unused(arg, UNUSED_ARG) && is_unused(arg_cageid, UNUSED_ID);
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
    if is_unused(cageid_1, UNUSED_ID) || is_unused(cageid_2, UNUSED_ID) || cageid_1 < 0 || cageid_2 < 0
    {
        return false;
    }
    true
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

    // Check if the upper 32 bits are all 0,
    // if so, we can safely convert it to u32
    // Otherwise, we will panic
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

/// This function translates a memory address from the WASM environment (user space)
/// to the corresponding host system address (kernel space). It is typically used when
/// the guest application passes a pointer argument to a syscall, and we need to dereference
/// it in the kernel context.
/// 
/// Input:
///     - addr_arg: the raw 64-bit address from the user
///     - addr_arg_cageid: the cage ID where the address belongs to
///     - cageid: the current running cage's ID (used for checking context)
/// 
/// Output:
///     - Returns a mutable pointer to host memory corresponding to the given address
///       from the guest. The pointer can be used for direct read/write operations.
pub fn sc_convert_addr_to_host(addr_arg: u64, addr_arg_cageid: u64, cageid: u64) -> *mut u8 {
    let cage = get_cage(addr_arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, addr_arg).unwrap() as *mut u8;
    return addr;
}

/// This function translates a buffer pointer from the WASM environment to a host pointer. 
/// It is typically used when a syscall needs to read a buffer (e.g., in `read`, `write`, etc).
///
/// Input:
///     - buf_arg: the raw address of the buffer in WASM space
///     - buf_arg_cageid: the cage ID of the buffer address
///     - cageid: current running cage ID
///
/// Output:
///     - Returns a constant (read-only) host pointer to the translated buffer.
///       Suitable for syscalls that only read from the buffer.
pub fn sc_convert_buf_to_host(buf_arg: u64, buf_arg_cageid: u64, cageid: u64) -> *const u8 {
    let cage = get_cage(buf_arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, buf_arg).unwrap() as *mut u8;
    return addr;
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

    // Check if the upper 32 bits are all 0,
    // if so, we can safely convert it to u32
    // Otherwise, we will panic
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

/// Convert a raw `u64` argument into a mutable `*mut u8` pointer, with optional
/// cage ID validation.
/// 
/// ## Arguments
/// - `arg`: The raw 64-bit value to be interpreted as a pointer.
/// - `arg_cageid`: Cage ID associated with the argument (source).
/// - `cageid`: Cage ID of the calling context (expected).
///
/// ## Returns
/// - A mutable pointer `*mut u8` corresponding to the given argument.
pub fn sc_convert_to_u8_mut(arg: u64, arg_cageid: u64, cageid: u64) -> *mut u8 {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }
    
    arg as *mut u8
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

/// Checks whether a user-space argument is null.
pub fn sc_convert_arg_nullity(arg: u64, arg_cageid: u64, cageid: u64) -> bool {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }
    
    (arg as *const u8).is_null()
}

/// Converts a user-space pointer into a mutable reference to `SockPair`.
pub fn sc_convert_sockpair<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> Result<&'a mut SockPair, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let cage = get_cage(arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    let pointer = addr as *mut SockPair;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

/// Converts a user-space socket address into a host-compatible `sockaddr` used for syscalls.
pub fn sc_convert_host_sockaddr(arg: u64, arg_cageid: u64, cageid: u64) -> (*mut sockaddr, u32) {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let mut saddr = SockAddr::clone_to_sockaddr(arg);

    if (saddr.sun_family as i32) == AF_UNIX {
        unsafe {
            let sun_path_ptr = saddr.sun_path.as_mut_ptr();
            let path_len = strlen(sun_path_ptr);
            let lind_root_len = LIND_ROOT.len();
            let new_path_len = path_len + lind_root_len;

            if new_path_len < 108 {
                memmove(
                    sun_path_ptr.add(lind_root_len) as *mut c_void,
                    sun_path_ptr as *const c_void,
                    path_len,
                );
                memcpy(
                    sun_path_ptr as *mut c_void,
                    LIND_ROOT.as_ptr() as *const c_void,
                    lind_root_len,
                );
                memset(
                    sun_path_ptr.add(new_path_len) as *mut c_void,
                    0,
                    108 - new_path_len,
                );
            }
        }
    }
    let boxed = Box::new(saddr);
    let ptr = Box::into_raw(boxed) as *mut sockaddr_un;
    let ptr = ptr.cast::<sockaddr>();
    let len = unsafe { (*(ptr as *mut SockAddr)).get_len() };
    (ptr, len)
}

/// Copies a socket address structure from the kernel into user space based on the given address family.
pub fn sc_convert_copy_out_sockaddr(
    addr_arg: u64,    
    addr_arg1: u64,   
    family: u16,
) {
    let copyoutaddr = addr_arg as *mut u8; // libc
    let addrlen = addr_arg1 as *mut u32;

    assert!(!copyoutaddr.is_null());
    assert!(!addrlen.is_null());

    let initaddrlen = unsafe { *addrlen };

    let (src_ptr, actual_len): (*const u8, u32) = match family as i32 {
        AF_INET => {
            let v4 = SockAddr::new_ipv4(); // self define
            (
                &v4 as *const _ as *const u8,
                size_of::<sockaddr_in>() as u32,
            )
        }
        AF_INET6 => {
            let v6 = SockAddr::new_ipv6();
            (
                &v6 as *const _ as *const u8,
                size_of::<sockaddr_in6>() as u32,
            )
        }
        AF_UNIX => {
            let un = SockAddr::new_unix();
            (
                &un as *const _ as *const u8,
                size_of::<sockaddr_un>() as u32,
            )
        }
        _ => return, 
    };

    let copy_len = initaddrlen.min(actual_len);
    unsafe {
        ptr::copy(src_ptr, copyoutaddr, copy_len as usize);
        *addrlen = actual_len.max(copy_len);
    }
}

