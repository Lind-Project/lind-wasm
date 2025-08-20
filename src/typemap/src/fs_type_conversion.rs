//! File system's type conversion related API
//!
//! This file provides APIs for converting between different argument types and translation between path from
//! user's perspective to host's perspective
use cage;
pub use libc::*;
use std::env;
pub use std::ffi::{CStr, CString};
use std::path::Component;
use std::path::PathBuf;
pub use std::{mem, ptr};
pub use sysdefs::constants::fs_const;
use cage::get_cage;
use cage::translate_vmmap_addr;
use crate::syscall_type_conversion::get_cstr;

/// If the `LIND_ROOT` environment variable is present at compile time, this will expand into an expression
/// of type Option<&'static str> whose value is Some of the value of the environment variable (a compilation
/// error will be emitted if the environment variable is not a valid Unicode string). If the environment
/// variable is not present, then this will expand to None, and will be set to default path.
pub const LIND_ROOT: &str = match option_env!("LIND_ROOT") {
    Some(path) => path,
    None => "/home/alice/lind-wasm/src/RawPOSIX/tmp",
};

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
    let addr = translate_vmmap_addr(&cage, path_arg).unwrap();
    let path = match get_cstr(addr) {
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
