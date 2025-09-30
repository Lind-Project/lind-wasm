//! Basic primitive type conversion API
//!
//! This file defines conversion helpers for basic primitive types (e.g., `i32`, `u32`, `i64`).
//! These functions are used during syscall argument decoding and type-safe interpretation
//! within the RawPOSIX syscall layer (`src/syscalls/`).
//! Function naming convention:
//! - All functions starting with `sc_` are **public APIs** exposed to other libraries. Example: `sc_convert_sysarg_to_i32`.
//! - All other functions are **internal helpers** (inner functions) used only inside this library.
use cage::{get_cage, memory::memory::translate_vmmap_addr};
use std::error::Error;
use std::str::Utf8Error;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, UNUSED_NAME, MAX_CAGEID, PATH_MAX};
use sysdefs::data::fs_struct::{SigactionStruct, SigsetType, ITimerVal, StatData, FSData, PipeArray, EpollEvent};
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

pub fn sc_convert_sysarg_to_i32_ref<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> &'a mut i32 {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    let cage = get_cage(cageid).unwrap();
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
    let cage = get_cage(cageid).unwrap();
    // Convert user buffer address to system address. We don't need to check permission here.
    // Permission check has been handled in 3i
    let buf = translate_vmmap_addr(&cage, buf_arg).unwrap() as *const u8;
    buf
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
pub fn sc_convert_uaddr_to_host(uaddr_arg: u64, uaddr_arg_cageid: u64, cageid: u64) -> u64 {
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
    let cage = get_cage(cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, addr_arg).unwrap() as *mut u8;
    return addr;
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

    // Get cage reference to translate address
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => return None,
    };

    // Convert user buffer address to system address. We don't need to check permission here.
    let addr = match translate_vmmap_addr(&cage, act_arg) {
        Ok(a) => a,
        Err(_) => return None,
    };

    let ptr = addr as *const SigactionStruct;
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
    // Get cage reference to translate address
    let cage = match get_cage(cageid) {
        Some(c) => c,
        None => return None,
    };
    // Convert user buffer address to system address. We don't need to check permission here.
    let addr = match translate_vmmap_addr(&cage, act_arg) {
        Ok(a) => a,
        Err(_) => return None,
    };

    let ptr = addr as *mut SigactionStruct;
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
pub fn sc_convert_sigset(set_arg: u64, set_cageid: u64, cageid: u64) -> Option<&'static mut SigsetType> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(set_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }

    if set_arg == 0 {
        return None; // If the argument is 0, return None
    } else {
        let cage = get_cage(cageid).unwrap();
        match translate_vmmap_addr(&cage, set_arg) {
            Ok(addr) => {
                let ptr = addr as *mut SigsetType;
                if !ptr.is_null() {
                    unsafe { return Some(&mut *ptr); }
                } else {
                    panic!("Failed to get SigsetType from address");
                }
            }
            Err(_) => panic!("Failed to get SigsetType from address"), // If translation fails, return None
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

    let cage = get_cage(cageid).unwrap();

    if val_arg == 0 {
        None
    } else {
        match translate_vmmap_addr(&cage, val_arg) {
            Ok(addr) => match get_constitimerval(addr) {
                Ok(itimeval) => itimeval,
                Ok(None) => None,
                Err(_) => panic!("Failed to get ITimerVal from address"),
            },
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

    let cage = get_cage(cageid).unwrap();

    if val_arg == 0 {
        None
    } else {
        match translate_vmmap_addr(&cage, val_arg) {
            Ok(addr) => match get_itimerval(addr) {
                Ok(itimeval) => itimeval,
                Ok(None) => None,
                Err(_) => panic!("Failed to get ITimerVal from address"),
            },
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
///  - The Cage object is retrieved via `get_cage(cageid)`.
///  - The virtual address is translated into a host address using
///    `translate_vmmap_addr`.
///  - The host address is cast into a `*mut StatData`, and if non-null,
///    reinterpreted as a mutable reference.
///  - If the pointer is null, the function returns `Err(Errno::EFAULT)`,
///    indicating a "Bad address" error consistent with Linux error handling.
///
/// ## Return Value:
///  - `Ok(&mut StatData)` if the address translation succeeds.
///  - `Err(Errno::EFAULT)` if the address is invalid or null.
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

    let cage = get_cage(cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    let pointer = addr as *mut StatData;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(Errno::EFAULT);
}

/// Translates a user-provided address from the Cage's virtual memory into
/// a mutable reference to an `FSData` structure.
///
/// This function follows the same logic as `sc_convert_addr_to_statdata`
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

    let cage = get_cage(cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    let pointer = addr as *mut FSData;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(Errno::EFAULT);
}

/// Translates a user-provided address from the Cage's virtual memory into
/// a mutable reference to a `PipeArray`.
///
/// This function follows the same pattern as other `sc_convert_addr_*`
/// helpers:
/// - Validates the Cage ID when the `secure` feature is enabled.
/// - Retrieves the Cage object via `get_cage`.
/// - Translates the Wasm linear memory address to a host address using
///   `translate_vmmap_addr`.
/// - Casts the host address to a `*mut PipeArray` and returns it as a
///   mutable reference if non-null.
/// - Returns `Err(Errno::EFAULT)` if the pointer is null, consistent with
///   Linux's `EFAULT` ("Bad address") error semantics.
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

    let cage = get_cage(cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    let pointer = addr as *mut PipeArray;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(Errno::EFAULT);
}
