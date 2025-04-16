//! Complex data structure type conversion API
//!
//! This file provides essential functions for handling and validating `u64` inputs, converting
//! them to various system-specific data types needed in system calls.  It includes utilities
//! for transforming raw pointers to typed structures, such as complex structures like polling,
//! signal handling, timing, and socket-related types.
use crate::syscall_conv::validate_cageid;
use sysdefs::data::fs_struct::PipeArray;
use sysdefs::data::net_struct::create_sockaddr_un;
use libc::*;
use std::ptr;

/// Convert a raw argument pointer into a mutable reference to a `PipeArray`
///
/// This function interprets the given `u64` value as a raw pointer to a `PipeArray`,
/// and attempts to return a mutable reference to it. It is typically used in
/// syscall argument decoding, where the raw argument comes from Wasm and needs to be
/// reinterpreted into a Rust type.
///
/// ## Arguments:
/// arg: a pointer to `PipeArray`
///
/// ## Returns:
/// `Ok(&mut PipeArray)` if the pointer is non-null and safely transmutable
/// `Err(i32)` (with `EFAULT`) if the pointer is null, indicating an invalid memory reference
pub fn get_pipearray<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> Result<&'a mut PipeArray, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            panic!("Invalide Cage ID");
        }
    }
    let pointer = arg as *mut PipeArray;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "typemap",
        "input data not valid",
    ));
}

/// converts a provided sockaddr pointer to a local `libc::sockaddr` pointer,
/// and returns the pointer and its length(for ipv4, ipv6). If the socket is UNIX, the path
/// is modified to include a LIND_ROOT prefix.
pub fn get_sockaddr(addr: *mut u8) -> (*mut libc::sockaddr, u32) {
    let (finalsockaddr, addrlen) = if addr.is_null() {
        // handle sockaddr conversion; if NULL, use empty pointer
        (ptr::null::<libc::sockaddr_un>() as *const libc::sockaddr_un as *mut libc::sockaddr, 0)
    } else {
        // create a new sockaddr_un structure to hold the copied data
        let mut new_sockaddr_struct = create_sockaddr_un();

        // get a mutable pointer to that structure
        let sockaddr_un_ptr: *mut sockaddr_un = &mut new_sockaddr_struct;

        unsafe {
            // copy user's sockaddr to local buffer
            ptr::copy_nonoverlapping(addr as *mut libc::sockaddr_un, sockaddr_un_ptr, 1);

            // if AF_UNIX socket, rewrite sun_path with LIND_ROOT prefix
            if (*sockaddr_un_ptr).sun_family as i32 == AF_UNIX {
                // get a mutable pointer to the beginning of the sun_path array
                let sun_path_ptr = (*sockaddr_un_ptr).sun_path.as_mut_ptr();

                // compute the original path length
                let path_len = libc::strlen(sun_path_ptr);

                // get the length of LIND_ROOT prefix we want to insert
                let lind_root_len: usize = LIND_ROOT.len();

                // total new path length = LIND_ROOT + original path
                let new_path_len = path_len + lind_root_len;
            
                // check if new path still fits within the 108-byte sun_path limit
                if new_path_len < 108 {
                    // move the original path forward in memory by lind_root_len bytes
                    // make space for LIND_ROOT
                    libc::memmove(
                        sun_path_ptr.add(lind_root_len) as *mut libc::c_void,
                        sun_path_ptr as *const libc::c_void,
                        path_len,
                    );
                    // copy the LIND_ROOT prefix into the beginning of sun_path
                    libc::memcpy(
                        sun_path_ptr as *mut libc::c_void,
                        LIND_ROOT.as_ptr() as *const libc::c_void,
                        lind_root_len,
                    );
                    // clean the rest of sun_path after the new content
                    libc::memset(
                        sun_path_ptr.add(new_path_len) as *mut libc::c_void,
                        0,
                        108 - new_path_len,
                    );
                }
            }
        }
        // return the pointer to our local sockaddr_un, cast to generic sockaddr,
        // along with its size
        (sockaddr_un_ptr as *mut libc::sockaddr, size_of::<libc::sockaddr_un>() as u32)
    };

    (finalsockaddr, addrlen)
}
