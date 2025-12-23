//! Basic primitive type conversion API
//!
//! This file defines conversion helpers for basic primitive types (e.g., `i32`, `u32`, `i64`).
//! These functions are used during syscall argument decoding and type-safe interpretation
//! within the RawPOSIX syscall layer (`src/syscalls/`).
//! Function naming convention:
//! - All functions starting with `sc_` are **public APIs** exposed to other libraries. Example: `sc_convert_sysarg_to_i32`.
//! - All other functions are **internal helpers** (inner functions) used only inside this library.
use cage::get_cage;
use std::error::Error;
use sysdefs::constants::lind_platform_const::{MAX_CAGEID, PATH_MAX};
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, UNUSED_NAME};
use sysdefs::constants::Errno;
use sysdefs::data::fs_struct::{
    FSData, ITimerVal, PipeArray, ShmidsStruct, SigactionStruct, SigsetType, StatData,
};

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
    if is_unused(cageid_1, UNUSED_ID)
        || is_unused(cageid_2, UNUSED_ID)
        || cageid_1 < 0
        || cageid_2 < 0
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

    // Check if the upper 32 bits are all 0,
    // if so, we can safely convert it to u32
    // Otherwise, we will panic
    if (arg & 0xFFFFFFFF_00000000) != 0 {
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
    if (arg & 0xFFFFFFFF_00000000) != 0 {
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

    unsafe { &mut *(arg as *mut i32) }
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
    buf_arg as *const u8
}

// TODO: This function can be removed/revamped significantly
// Leaving it in for now since it is used threei/
/// ## Arguments:
/// - `uaddr`: The user address to convert (u64).
/// - `addr_cageid`: The cage ID associated with the address.
/// - `cageid`: The calling cage ID (used for validation in secure mode).
///
/// ## Returns:
/// - The host address as u64, or 0 if the address is null.
pub fn sc_convert_uaddr_to_host(uaddr: u64, addr_cageid: u64, cageid: u64) -> u64 {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(addr_cageid, cageid) {
            return 0;
        }
    }

    let cage = get_cage(addr_cageid).unwrap();
    let vmmap = cage.vmmap.read();
    let base_addr = vmmap.base_address.unwrap() as u64;

    if uaddr < base_addr {
        return uaddr + base_addr;
    }

    uaddr
}

/// Translates a user-provided address from the Cage's virtual memory into
/// a mutable reference to a `libc::epoll_event`.
///
/// This function follows the same pattern as other `sc_convert_addr_*`
/// helpers:
/// - Validates the Cage ID when the `secure` feature is enabled.
/// - Casts the address to a `*mut libc::epoll_event` and returns it as a
///   mutable reference.
///
/// Note: Null pointer validation is now performed at the glibc layer before
/// calling into rawposix, so this function assumes the pointer is valid.
///
/// The libc::epoll_event structure matches the kernel's struct epoll_event:
/// - `events: u32` - event mask
/// - `u64: u64` - union data field (can represent fd, ptr, u32, or u64)
pub fn sc_convert_addr_to_epollevent<'a>(
    arg: u64,
    arg_cageid: u64,
    cageid: u64,
) -> Result<&'a mut libc::epoll_event, Errno> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let pointer = arg as *mut libc::epoll_event;
    Ok(unsafe { &mut *pointer })
}

/// Convert a user-provided pointer (u64) from a cage into a shared reference to
/// a `SigactionStruct`.
///
/// # Arguments
/// * `act_arg` - The raw user pointer (u64). If `0`, this means "no struct".
/// * `act_arg_cageid` - The cage ID in which the pointer resides.
/// * `cageid` - The caller’s cage ID (can be used for cross-cage checks).
///
/// # Returns
/// * `Some(&SigactionStruct)` if the pointer is nonzero and translation succeeds.
/// * `None` if `act_arg == 0`.
pub fn sc_convert_sigactionStruct<'a>(
    act_arg: u64,
    act_arg_cageid: u64,
    cageid: u64,
) -> Option<&'a SigactionStruct> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(act_arg_cageid, cageid) {
            return None;
        }
    }
    // If we don't have act arg, return None
    if act_arg == 0 {
        return None;
    }

    let ptr = act_arg as *const SigactionStruct;
    unsafe { Some(&*ptr) }
}

/// Convert a user-provided pointer (u64) from a cage into a mutable reference to
/// a `SigactionStruct`.
///
/// # Arguments
/// * `act_arg` - The raw user pointer (u64). If `0`, this means "no struct".
/// * `act_arg_cageid` - The cage ID in which the pointer resides.
/// * `cageid` - The caller’s cage ID (can be used for cross-cage checks).
///
/// # Returns
/// * `Some(&mut SigactionStruct)` if the pointer is nonzero and translation succeeds.
/// * `None` if `act_arg == 0`.
pub fn sc_convert_sigactionStruct_mut<'a>(
    act_arg: u64,
    act_arg_cageid: u64,
    cageid: u64,
) -> Option<&'a mut SigactionStruct> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(act_arg_cageid, cageid) {
            return None;
        }
    }
    // If we don't have act arg, return None
    if act_arg == 0 {
        return None;
    }

    let ptr = act_arg as *mut SigactionStruct;
    unsafe { Some(&mut *ptr) }
}

/// Convert a user-provided pointer (u64) from a cage into a mutable reference to
/// a `SigsetType`.
///
/// # Arguments
/// * `set_arg` - The raw user pointer (u64). If `0`, this means "no struct".
/// * `set_arg_cageid` - The cage ID in which the pointer resides.
/// * `cageid` - The caller’s cage ID (can be used for cross-cage checks).
///
/// # Returns
/// * `Some(&mut SigsetType)` if the pointer is nonzero and translation succeeds.
/// * `None` if `set_arg == 0`.
pub fn sc_convert_sigset(
    set_arg: u64,
    set_cageid: u64,
    cageid: u64,
) -> Option<&'static mut SigsetType> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(set_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    if set_arg == 0 {
        return None; // If the argument is 0, return None
    } else {
        let ptr = set_arg as *mut SigsetType;
        if !ptr.is_null() {
            unsafe {
                return Some(&mut *ptr);
            }
        } else {
            panic!("Failed to get SigsetType from address");
        }
    }
}

/// Convert a raw u64 address into a mutable reference to an `ITimerVal`.
///
/// # Arguments
/// * `addr` – Raw address pointing to an `ITimerVal` structure.
///
/// # Returns
/// * `Ok(Some(&mut ITimerVal))` if `addr` is non-null and successfully converted.
/// * `Err(-1)` if `addr` is null.
pub fn get_itimerval<'a>(addr: u64) -> Result<Option<&'a mut ITimerVal>, i32> {
    let ptr = addr as *mut ITimerVal;
    if !ptr.is_null() {
        unsafe {
            return Ok(Some(&mut *ptr));
        }
    }
    Err(-1)
}

/// Convert a raw u64 address into an immutable reference to an `ITimerVal`.
///
/// # Arguments
/// * `addr` – Raw address pointing to an `ITimerVal` structure.
///
/// # Returns
/// * `Ok(Some(&ITimerVal))` if `addr` is non-null and successfully converted.
/// * `Err(-1)` if `addr` is null.
pub fn get_constitimerval<'a>(addr: u64) -> Result<Option<&'a ITimerVal>, i32> {
    let ptr = addr as *const ITimerVal;
    if !ptr.is_null() {
        unsafe {
            return Ok(Some(&*ptr));
        }
    }
    Err(-1)
}

/// Converts a memory address to a constant `SigsetType` (u64).
///
/// # Arguments
/// * `addr` – Raw memory address pointing to a SigsetType.
///
/// # Returns
/// * `Ok(SigsetType)` if `addr` is valid and successfully converted.
/// * `Err(-1)` if `addr` is null.
pub fn get_constsigset(addr: u64) -> Result<SigsetType, i32> {
    let ptr = addr as *const SigsetType;
    if !ptr.is_null() {
        unsafe {
            return Ok(*ptr);
        }
    }
    Err(-1)
}

/// Translate a user-provided argument into an immutable reference to an `ITimerVal`.
///
/// This is a higher-level wrapper that first resolves the cage memory mapping,
/// then converts the address into a reference using [`get_constitimerval`].
///
/// # Arguments
/// * `val_arg` – User-provided raw pointer (u64).
/// * `val_arg_cageid` – The cage ID in which `val_arg` resides.
/// * `cageid` – The calling cage ID (used for optional validation).
///
/// # Returns
/// * `Some(ITimerVal)` if `val_arg` is nonzero and translation succeeds.
/// * `None` if `val_arg == 0`.
///
/// # Panics
/// * If cage lookup or address translation fails.
/// * If conversion to `ITimerVal` fails.
pub fn sc_convert_itimerval(
    val_arg: u64,
    val_arg_cageid: u64,
    cageid: u64,
) -> Option<&'static ITimerVal> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(val_arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    if val_arg == 0 {
        None
    } else {
        match get_constitimerval(val_arg) {
            Ok(itimeval) => itimeval,
            Ok(None) => None,
            Err(_) => panic!("Failed to get ITimerVal from address"),
        }
    }
}

/// Translate a user-provided argument into a mutable reference to an `ITimerVal`.
///
/// This is a higher-level wrapper that first resolves the cage memory mapping,
/// then converts the address into a mutable reference using [`get_itimerval`].
///
/// # Arguments
/// * `val_arg` – User-provided raw pointer (u64).
/// * `val_arg_cageid` – The cage ID in which `val_arg` resides.
/// * `cageid` – The calling cage ID (used for optional validation).
///
/// # Returns
/// * `Some(&mut ITimerVal)` if `val_arg` is nonzero and translation succeeds.
/// * `None` if `val_arg == 0`.
///
/// # Panics
/// * If cage lookup or address translation fails.
/// * If conversion to `ITimerVal` fails.
pub fn sc_convert_itimerval_mut(
    val_arg: u64,
    val_arg_cageid: u64,
    cageid: u64,
) -> Option<&'static mut ITimerVal> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(val_arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    if val_arg == 0 {
        None
    } else {
        match get_itimerval(val_arg) {
            Ok(itimeval) => itimeval,
            Ok(None) => None,
            Err(_) => panic!("Failed to get ITimerVal from address"),
        }
    }
}

/// `sc_convert_addr_to_statdata` translates a user-provided address from the
/// calling Cage's virtual memory into a mutable reference to a `StatData`
/// structure.
///
/// ## Arguments:
///  - `arg`: The raw virtual address of the user-provided `StatData` buffer.
///  - `arg_cageid`: The Cage ID associated with the provided address.
///  - `cageid`: The Cage ID of the current caller (used for validation).
///
/// ## Implementation Details:
///  - When the `secure` feature is enabled, the function validates that the
///    provided `arg_cageid` matches the current `cageid`.
///  - The address is cast into a `*mut StatData` and returned as a mutable reference.
///
/// Note: Null pointer validation is now performed at the glibc layer before
/// calling into rawposix, so this function assumes the pointer is valid.
///
/// ## Return Value:
///  - `Ok(&mut StatData)` if the address translation succeeds.
pub fn sc_convert_addr_to_statdata<'a>(
    arg: u64,
    arg_cageid: u64,
    cageid: u64,
) -> Result<&'a mut StatData, Errno> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let pointer = arg as *mut StatData;
    Ok(unsafe { &mut *pointer })
}

/// Translates a user-provided address from the Cage's virtual memory into
/// a mutable reference to an `FSData` structure.
///
/// This function follows the same logic as `sc_convert_addr_to_statdata`.
///
/// Note: Null pointer validation is now performed at the glibc layer before
/// calling into rawposix, so this function assumes the pointer is valid.
pub fn sc_convert_addr_to_fstatdata<'a>(
    arg: u64,
    arg_cageid: u64,
    cageid: u64,
) -> Result<&'a mut FSData, Errno> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let pointer = arg as *mut FSData;
    Ok(unsafe { &mut *pointer })
}

/// Translates a user-provided address from the Cage's virtual memory into
/// a mutable reference to a `PipeArray`.
///
/// This function follows the same pattern as other `sc_convert_addr_*`
/// helpers:
/// - Validates the Cage ID when the `secure` feature is enabled.
/// - Casts the address to a `*mut PipeArray` and returns it as a
///   mutable reference.
///
/// Note: Null pointer validation is now performed at the glibc layer before
/// calling into rawposix, so this function assumes the pointer is valid.
pub fn sc_convert_addr_to_pipearray<'a>(
    arg: u64,
    arg_cageid: u64,
    cageid: u64,
) -> Result<&'a mut PipeArray, Errno> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let pointer = arg as *mut PipeArray;
    Ok(unsafe { &mut *pointer })
}

/// Translates a user-provided address from the Cage's virtual memory into
/// a mutable reference to a `ShmidsStruct`.
///
/// This function mirrors the structure of other `sc_convert_addr_*`
/// helpers:
/// - Validates the Cage ID when the `secure` feature is enabled.
/// - Casts the address to a `*mut ShmidsStruct` and returns it as a
///   mutable reference.
///
/// Note: Null pointer validation is now performed at the glibc layer before
/// calling into rawposix, so this function assumes the pointer is valid.
pub fn sc_convert_addr_to_shmidstruct<'a>(
    arg: u64,
    arg_cageid: u64,
    cageid: u64,
) -> Result<&'a mut ShmidsStruct, Errno> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let pointer = arg as *mut ShmidsStruct;
    Ok(unsafe { &mut *pointer })
}

/// Converts a raw `u64` argument into a nullity check.
/// If the `secure` feature is enabled, this also validates that the argument’s
/// cage ID matches the current cage ID. If validation fails, the function
/// returns early with `-1`.
///
/// Otherwise, the argument is cast to a pointer and checked for null,
/// returning `true` if the argument is null and `false` if not.
pub fn sc_convert_arg_nullity(arg: u64, arg_cageid: u64, cageid: u64) -> bool {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    (arg as *const u8).is_null()
}
