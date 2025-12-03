//! File system's type conversion related API
//!
//! This file provides APIs for converting between different argument types and translation between path from
//! user's perspective to host's perspective
use cage::get_cage;
pub use libc::*;
pub use std::env;
pub use std::ffi::{CStr, CString};
pub use std::path::{Component, PathBuf};
use std::str::Utf8Error;
pub use std::{mem, ptr};
use sysdefs::constants::lind_platform_const::LIND_ROOT;
pub use sysdefs::constants::{err_const, fs_const};

/// Convert data type from `&str` to `PathBuf`
///
/// ## Argument:
/// cpath: a path string slice in type &str
///
/// ## Returns:
/// A `PathBuf` created from the input string.
pub fn convpath(cpath: &str) -> PathBuf {
    PathBuf::from(cpath)
}

/// Normalize receiving path arguments to eliminating `./..` and generate a canonicalized (but not
/// symlink-resolved) path. This function will adding the cage's current working directory at the
/// beginning, if given path argument is relative; or adding the virtual root `/` if given path
/// argument is absolute.
///
/// ## Arguments:
/// origp: path to normalize.
/// cageid: cage ID of the `origp`
///
/// ## Returns:
/// A `PathBuf` representing the normalized absolute path.
pub fn normpath(origp: PathBuf, cageid: u64) -> PathBuf {
    let cage = cage::get_cage(cageid).unwrap();
    //If path is relative, prefix it with the current working directory, otherwise populate it with rootdir
    let mut newp = if origp.is_relative() {
        (**cage.cwd.read()).clone()
    } else {
        PathBuf::from("/")
    };

    for comp in origp.components() {
        match comp {
            //if we have a normal path component, push it on to our normed path
            Component::Normal(_) => {
                newp.push(comp);
            }

            //if we have a .. path component, pop the last component off our normed path
            Component::ParentDir => {
                newp.pop();
            }

            //if we have a . path component (Or a root dir or a prefix(?)) do nothing
            _ => {}
        };
    }
    newp
}

/// This function first normalizes the path, then add `LIND_ROOT` at the beginning.
/// This function is mostly used by path argument translation function in `syscall_conv`
///
/// ## Arguments:
///     - cageid: used for normalizing path
///     - path: the user seen path
///
/// ## Returns:
///     - c_path: path location from host's perspective
pub fn add_lind_root(cageid: u64, path: &str) -> CString {
    // Convert data type from &str into *const i8
    let relpath = normpath(convpath(path), cageid);
    let relative_path = relpath.to_str().unwrap();

    let full_path = format!("{}{}", LIND_ROOT, relative_path);
    let c_path = CString::new(full_path).unwrap();
    c_path
}

/// Remove LIND_ROOT prefix from a host path to convert it back to user perspective.
///
/// This function is the reverse of `add_lind_root`. It strips the LIND_ROOT prefix from an
/// absolute host path and returns the path as it should appear to the user (cage). This is
/// primarily used when retrieving paths from the kernel (e.g., via `getcwd()`) that need to
/// be stored in cage state or returned to user space.
///
/// ## Arguments:
///     - host_path: The full host path including LIND_ROOT prefix
///
/// ## Returns:
///     - PathBuf representing the path from user's perspective (without LIND_ROOT)
///
/// ## Example:
/// ```
/// // If LIND_ROOT is "/home/lind/lind-wasm/src/tmp"
/// // and host_path is "/home/lind/lind-wasm/src/tmp/foo/bar"
/// // this returns "/foo/bar"
/// let user_path = strip_lind_root("/home/lind/lind-wasm/src/tmp/foo/bar");
/// assert_eq!(user_path, PathBuf::from("/foo/bar"));
/// ```
pub fn strip_lind_root(host_path: &str) -> PathBuf {
    if let Ok(stripped) = PathBuf::from(host_path).strip_prefix(LIND_ROOT) {
        // Prepend "/" to make it an absolute path from user's perspective
        PathBuf::from("/").join(stripped)
    } else {
        // If path doesn't start with LIND_ROOT, return it as-is
        // This shouldn't normally happen but provides a fallback
        PathBuf::from(host_path)
    }
}

/// This function provides two operations: first, it translates path pointer address from WASM environment
/// to kernel system address; then, it adjusts the path from user's perspective to host's perspective,
/// which is adding `LIND_ROOT` before the path arguments. Considering actual syscall implementation
/// logic needs to pass string pointer to underlying rust libc, so this function will return `CString`
/// always using arg_cageid to translate. (TODO: the logic here might be different according to 3i/grate
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

/// Convert received path pointer into a normalized `CString` path in the host cage.
///
/// This function first validates cross-cage access if `secure` feature is enabled.
/// After translating the given path pointer from virtual address to the real address,
/// this function reads and normalizes the path relative to the cage's CWD or root.
/// Finally prefixes the path with the host-defined `LIND_ROOT`, then constructs a `CString`.
///
/// ## Arguments:
/// path_arg: virtual address of the path string
/// path_arg_cageid: The cage ID that owns the virtual address.
/// cageid: The cage ID making the system call
///
/// ## Returns:
/// A `CString` representing the absolute path in the host perspective (kernel perspective).
pub fn sc_convert_path_to_host(path_arg: u64, path_arg_cageid: u64, cageid: u64) -> CString {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(path_arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }
    let cage = get_cage(path_arg_cageid).unwrap();

    let path = match get_cstr(path_arg) {
        Ok(path) => path,
        Err(e) => panic!("{:?}", e),
    };
    // We will create a new variable in host process to handle the path value
    let relpath = normpath(convpath(path), path_arg_cageid);
    let relative_path = relpath.to_str().unwrap();

    // Check if exceeds the max path
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
