//! File system's type conversion related API
//!
//! This file provides APIs for converting between different argument types and translation between path from
//! user's perspective to host's perspective
use crate::cage_helpers::validate_cageid;
use cage::get_cage;
pub use libc::*;
pub use std::env;
pub use std::ffi::{CStr, CString};
pub use std::path::{Component, PathBuf};
use std::str::Utf8Error;
pub use std::{mem, ptr};
pub use sysdefs::constants::lind_platform_const::PATH_MAX;
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

/// This function provides two operations: first, it translates path pointer address from WASM environment
/// to kernel system address; then, it normalizes the path relative to the cage's current working directory
/// (for relative paths) or root (for absolute paths). The syscall implementation logic needs to pass a
/// string pointer to underlying rust libc, so this function returns `CString`, always using arg_cageid
/// to translate. (TODO: the logic here might be different according to 3i/grate implementation)
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

/// Convert received path pointer into a normalized `CString` path.
///
/// This function first validates cross-cage access if `secure` feature is enabled.
/// After translating the given path pointer from virtual address to the real address,
/// this function reads and normalizes the path relative to the cage's CWD or root,
/// then constructs a `CString` for use with libc syscalls.
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
            panic!("Invalid Cage ID");
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
        if relative_path.len() >= PATH_MAX {
            panic!("Path exceeds PATH_MAX (4096)");
        }
    }

    // CString will handle the case when string is not terminated by `\0`, but will return error if `\0` is
    // contained within the string.
    let full_path = relative_path.to_string();
    match CString::new(full_path) {
        Ok(c_path) => c_path,
        Err(_) => panic!("String contains internal null byte"),
    }
}
