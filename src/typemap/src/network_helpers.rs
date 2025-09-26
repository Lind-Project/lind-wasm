//! Network-related helpers
//!
//! This module provides helpers to translate a guest-provided sockaddr buffer into a 
//! host-usable pointer and to compute the correct socklen_t for Linux. It is used by 
//! our socket-related syscalls to bridge from per-cage virtual memory to host libc calls.
use sysdefs::data::net_struct::{SockAddr, SockPair};
use libc::{sockaddr, strlen, sockaddr_un, sockaddr_in, sockaddr_in6};
use sysdefs::constants::{Errno, syscall_error};
use sysdefs::constants::lind_platform_const::LIND_ROOT;
use sysdefs::constants::net_const::{AF_UNIX};
use cage::{get_cage, memory::memory::translate_vmmap_addr};
use crate::datatype_conversion::validate_cageid;
use std::os::raw::{c_void, c_char};
use std::ptr;

/// Compute the effective `socklen_t` for a Linux `AF_UNIX` address given its `sun_path`.
///
/// Linux length rules:
/// - The base is `offsetof(sockaddr_un, sun_path) == 2` because `sa_family_t` is 16-bit.
/// - In the case of first byte == 0:
///   length = base + index_of_last_nonzero_byte_in_sun_path + 1  (no trailing NULL is added)
/// - In the case of first byte != 0:
///   length = base + strlen(sun_path) + 1 (to include the terminating NULL)
///   If the 108-byte array has no NULL at all (completely full), use base + 108.
///
/// Why this exists:
/// Some syscalls (e.g., `bind`, `connect`) and ancillary logic need the address
/// length. Callers sometimes only have the 108-byte `sun_path` buffer; this helper applies
/// the kernel’s rules to produce the correct `socklen_t`.
unsafe fn unix_len_from_sun_path(sun_path: &[i8; 108]) -> libc::socklen_t {
    // offset of (sockaddr_un, sun_path) is 2, because sa_family_t is a 16-bit (u16) field
    let base: libc::socklen_t = 2;

    if sun_path[0] == 0 {
        // Find the last non-zero byte; if none, set `used = 0`
        // (`i` runs forward so `used` ends up `1 + last_nonzero_index`)
        let mut used = 0usize;
        for i in 0..108 {
            if sun_path[i] != 0 { used = i + 1; }
        }
        base + used as libc::socklen_t
    } else {
        let mut n = 0usize;
        // Count bytes until the first 0 or the end of the array.
        while n < 108 && sun_path[n] != 0 { n += 1; }
        // If we found a NULL inside the array, include it (+1).
        // If not (array is completely full), kernel takes the whole 108 without an extra NULL.
        let add_nul = if n < 108 { 1 } else { 0 };
        base + (n + add_nul) as libc::socklen_t
    }
}

/// `convert_host_sockaddr` first interprets the incoming pointer as a sockaddr buffer 
/// and clones just the bytes it needs into our internal `SockAddr` helper so we can 
/// safely inspect `sa_family` and, for `AF_UNIX`, stage any path rewriting without 
/// risking accidental corruption of the caller’s memory. That local `SockAddr` is 
/// used to decide what the correct `socklen_t` should be and, in the `AF_UNIX` case,
/// to compute and prepare the prefixed path. After that decision, the function 
/// performs any required edits in place on the original buffer (e.g., shifting the 
/// existing path, inserting the `LIND_ROOT` prefix, zero-filling the tail, and 
/// ensuring the family field is consistent), then returns the original pointer 
/// (now containing the modified bytes) together with the computed length.
pub fn convert_host_sockaddr(
    arg: *mut u8,
    arg_cageid: u64,
    cageid: u64,
) -> (*mut libc::sockaddr, libc::socklen_t) {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return (core::ptr::null_mut(), 0);
        }
    }

    // nothing to translate.
    if arg.is_null() {
        return (core::ptr::null_mut(), 0);
    }

    // Clone just enough bytes from the incoming buffer into our small helper,
    // so we can read `sa_family` and (for AF_UNIX) examine/prepare the path.
    let mut saddr = unsafe { SockAddr::clone_to_sockaddr(arg) };

    let mut out_len: libc::socklen_t = 0;

    if (saddr.sun_family as i32) == AF_UNIX {
        unsafe {
            // Point to the start of `sun_path` inside the *original* buffer.
            // On Linux, `sa_family_t` is u16, so `sun_path` is immediately after 2 bytes.
            let sun_path_ptr = (arg.add(size_of::<libc::sa_family_t>())) as *mut i8;

            // Current path length (for pathname form this is strlen; for abstract form this is 0).
            let path_len = strlen(sun_path_ptr);

            // We prefix with LIND_ROOT if it fits; compute the final length in bytes.
            let lind_root_len = LIND_ROOT.len();
            let new_path_len = path_len + lind_root_len;

            // Only rewrite in place if the prefixed path still fits into the 108-byte sun_path.
            if new_path_len < 108 {
                // Shift existing bytes forward to make room for the prefix.
                ptr::copy(
                    sun_path_ptr,
                    sun_path_ptr.add(lind_root_len),
                    path_len,
                );
                // Write the prefix at the start 
                ptr::copy_nonoverlapping(
                    LIND_ROOT.as_ptr(),
                    sun_path_ptr as *mut u8,
                    lind_root_len,
                );
                // Zero-fill the remaining tail
                ptr::write_bytes(
                    sun_path_ptr.add(new_path_len),
                    0,
                    108 - new_path_len,
                );

                // Keep our local mirror in sync for length calculation
                saddr.sun_path[..new_path_len]
                    .copy_from_slice(core::slice::from_raw_parts(sun_path_ptr, new_path_len));
                for b in &mut saddr.sun_path[new_path_len..] { *b = 0; }
            }

            // Ensure the family field at the head of the original buffer is consistent.
            ptr::write_unaligned(arg as *mut u16, saddr.sun_family);
        }

        out_len = unsafe { unix_len_from_sun_path(&saddr.sun_path) };
    } else {
        // Non-UNIX families: we don’t modify the buffer; length is the canonical sizeof(*).
        out_len = match saddr.sun_family as i32 {
            libc::AF_INET  => size_of::<libc::sockaddr_in>()  as libc::socklen_t,
            libc::AF_INET6 => size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            _              => size_of::<libc::sockaddr>()     as libc::socklen_t,
        };
    }

    (arg as *mut libc::sockaddr, out_len)
}

pub fn sc_convert_arg_nullity(arg: u64, arg_cageid: u64, cageid: u64) -> bool {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }
    
    (arg as *const u8).is_null()
}

pub fn copy_out_sockaddr(
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

pub fn convert_sockpair<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> Result<&'a mut SockPair, i32> {
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
