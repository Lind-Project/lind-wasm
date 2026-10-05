//! File system's type conversion related API
//!
//! This file provides APIs for converting between different argument types and translation between path from
//! user's perspective to host's perspective
use crate::cage_helpers::validate_cageid;
use cage::get_cage;
pub use libc::*;
pub use std::env;
pub use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
pub use std::path::{Component, PathBuf};
use std::str::Utf8Error;
pub use std::{mem, ptr};
use sysdefs::constants::err_const::Errno;
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

fn has_trailing_separator(path: &PathBuf) -> bool {
    let bytes = path.as_os_str().as_bytes();
    bytes.len() > 1 && bytes.ends_with(b"/")
}

fn preserve_trailing_separator(mut path: PathBuf, preserve: bool) -> PathBuf {
    if preserve && !has_trailing_separator(&path) && path.as_os_str().as_bytes() != b"/" {
        let mut os_path = path.into_os_string();
        os_path.push("/");
        path = PathBuf::from(os_path);
    }

    path
}

pub fn path_without_trailing_slashes(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/"
    } else {
        trimmed
    }
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
    let preserve_trailing_slash = has_trailing_separator(&origp);

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

    preserve_trailing_separator(newp, preserve_trailing_slash)
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

/// Like `get_cstr` but accepts non-UTF-8 data by using lossy conversion.
/// Used in the execve path where argv/envp may contain arbitrary bytes
/// (e.g. 8-bit delimiters in coreutils tests).
pub fn get_cstr_lossy(arg: u64) -> Result<String, i32> {
    let ptr = arg as *const std::ffi::c_char;
    if !ptr.is_null() {
        let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
        return Ok(cstr.to_string_lossy().into_owned());
    }
    Err(-1)
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
pub fn sc_convert_path_to_host(
    path_arg: u64,
    path_arg_cageid: u64,
    cageid: u64,
) -> Result<CString, Errno> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(path_arg_cageid, cageid) {
            panic!("Invalid Cage ID");
        }
    }

    let path = match get_cstr(path_arg) {
        Ok(path) => path,
        Err(_) => return Err(Errno::EFAULT),
    };
    // We will create a new variable in host process to handle the path value
    let relpath = normpath(convpath(path), path_arg_cageid);
    let relative_path = match relpath.to_str() {
        Some(s) => s,
        None => return Err(Errno::EINVAL),
    };

    // Check if exceeds the max path
    #[cfg(feature = "secure")]
    {
        if relative_path.len() >= PATH_MAX {
            return Err(Errno::ENAMETOOLONG);
        }
    }

    // CString will handle the case when string is not terminated by `\0`, but will return error if `\0` is
    // contained within the string.
    let full_path = relative_path.to_string();
    match CString::new(full_path) {
        Ok(c_path) => Ok(c_path),
        Err(_) => return Err(Errno::EINVAL),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_trailing_separator_on_normalized_non_root_path() {
        let path = preserve_trailing_separator(PathBuf::from("/tmp/file"), true);
        assert_eq!(path.to_str(), Some("/tmp/file/"));
    }

    #[test]
    fn does_not_add_trailing_separator_without_original_separator() {
        let path = preserve_trailing_separator(PathBuf::from("/tmp/file"), false);
        assert_eq!(path.to_str(), Some("/tmp/file"));
    }

    #[test]
    fn does_not_add_extra_separator_to_root() {
        let path = preserve_trailing_separator(PathBuf::from("/"), true);
        assert_eq!(path.to_str(), Some("/"));
    }

    #[test]
    fn path_without_trailing_slashes_removes_extra_slashes() {
        assert_eq!(path_without_trailing_slashes("/tmp/dir/"), "/tmp/dir");
        assert_eq!(path_without_trailing_slashes("/tmp/dir///"), "/tmp/dir");
    }

    #[test]
    fn path_without_trailing_slashes_preserves_root() {
        assert_eq!(path_without_trailing_slashes("/"), "/");
        assert_eq!(path_without_trailing_slashes("///"), "/");
    }

    use cage::DashMap;

    // Builds a minimal cage and registers it under 'cageid', so
    // normpath(path, cageid) can find it via cage::get_cage()
    // Every field besides 'cwd' is irrelevant to path logic, they're
    // filled with empty/default values to satisfy the struct
    fn make_test_cage(cageid: u64, cwd: &str) {
        cage::cagetable_init();

        let test_cage = cage::Cage {
            cageid,
            parent: cageid,
            cwd: cage::RwLock::new(cage::Arc::new(PathBuf::from(cwd))),
            rev_shm: cage::Mutex::new(Vec::new()),
            signalhandler: DashMap::new(),
            sigset: cage::AtomicU64::new(0),
            pending_signals: cage::RwLock::new(vec![]),
            epoch_handler: DashMap::new(),
            os_tid_map: DashMap::new(),
            main_threadid: cage::RwLock::new(0),
            interval_timer: cage::IntervalTimer::new(cageid),
            zombies: cage::RwLock::new(vec![]),
            child_num: cage::AtomicU64::new(0),
            vmmap: cage::RwLock::new(cage::Vmmap::new()),
            final_exit_status: cage::RwLock::new(None),
            exit_group_initiated: cage::AtomicBool::new(false),
            is_dead: cage::AtomicBool::new(false),
            grate_inflight: cage::AtomicU64::new(0),
        };
        cage::add_cage(cageid, test_cage);
    }
    #[test]
    //ISO-004: an absolute path must always be rebuilt from the virtual root
    // never passed through some other branch unmodified
    fn normpath_confines_absolute_path_to_virtual_root() {
        let cageid = 1500;
        make_test_cage(cageid, "/");

        let result = normpath(PathBuf::from("/etc/../etc/passwd"), cageid);

        assert_eq!(result, PathBuf::from("/etc/passwd"));
    }

    #[test]
    //ISO-004: excess ".." must clamp at the virtual root instead of
    // going negative even when climbing from a real nested cwd
    fn normpath_clamps_excess_parent_dir_at_root() {
        let cageid = 1501;
        make_test_cage(cageid, "/home/user/project");

        let result = normpath(PathBuf::from("../../../../../../etc/passwd"), cageid);
        assert_eq!(result, PathBuf::from("/etc/passwd"));
    }

    #[test]
    // ISO-004: ordinary ".." must still resolve correctly, not just get
    // clamped away, proves the clamp isn't overly aggressive
    fn normpath_resolves_ordinary_parent_dir_correctly() {
        let cageid = 1502;
        make_test_cage(cageid, "/a/b");

        let result = normpath(PathBuf::from("foo/../../bar"), cageid);
        assert_eq!(result, PathBuf::from("/a/bar"));
    }
}
