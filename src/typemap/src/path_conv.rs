use cage;
pub use libc::*;
pub use std::ffi::{CStr, CString};
/// Path conversion related API
///
/// This file provides APIs for converting between different argument types and translation between path from
/// user's perspective to host's perspective
use std::path::Component;
use std::path::PathBuf;
pub use std::{mem, ptr};

pub use sysdefs::constants::fs_const;

use std::env;
/// If the `LIND_ROOT` environment variable is present at compile time, this will expand into an expression
/// of type Option<&'static str> whose value is Some of the value of the environment variable (a compilation
/// error will be emitted if the environment variable is not a valid Unicode string). If the environment
/// variable is not present, then this will expand to None, and will be set to default path.
pub const LIND_ROOT: &str = match option_env!("LIND_ROOT") {
    Some(path) => path,
    None => "/home/lind/lind-wasm/src/RawPOSIX/tmp",
};

/// Convert data type from `&str` to `PathBuf`
pub fn convpath(cpath: &str) -> PathBuf {
    PathBuf::from(cpath)
}

/// Normalize receiving path arguments to eliminating `./..` and generate a
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
/// Input:
///     - cageid: used for normalizing path
///     - path: the user seen path
///
/// Output:
///     - c_path: path location from host's perspective
pub fn add_lind_root(cageid: u64, path: &str) -> CString {
    // Convert data type from &str into *const i8
    let relpath = normpath(convpath(path), cageid);
    let relative_path = relpath.to_str().unwrap();

    let full_path = format!("{}{}", LIND_ROOT, relative_path);
    let c_path = CString::new(full_path).unwrap();
    c_path
}
