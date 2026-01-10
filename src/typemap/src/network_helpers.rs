//! Network-related helpers
//!
//! This module provides helpers to translate a guest-provided sockaddr buffer into a
//! host-usable pointer and to compute the correct socklen_t for Linux. It is used by
//! our socket-related syscalls to bridge from per-cage virtual memory to host libc calls.
use crate::datatype_conversion::validate_cageid;
use cage::get_cage;
use libc::{
    sa_family_t, sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage, sockaddr_un, socklen_t,
    strlen,
};
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::ptr;
use sysdefs::constants::lind_platform_const::LIND_ROOT;
use sysdefs::constants::net_const::AF_UNIX;
use sysdefs::constants::{syscall_error, Errno};
use sysdefs::data::net_struct::{SockAddr, SockPair};

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
            if sun_path[i] != 0 {
                used = i + 1;
            }
        }
        base + used as libc::socklen_t
    } else {
        let mut n = 0usize;
        // Count bytes until the first 0 or the end of the array.
        while n < 108 && sun_path[n] != 0 {
            n += 1;
        }
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

            // Extract guest path to determine if it's relative or absolute
            let guest_path_str = if path_len > 0 {
                match CStr::from_ptr(sun_path_ptr).to_str() {
                    Ok(s) => s,
                    Err(_) => "",
                }
            } else {
                ""
            };

            // Build host path with proper separator
            let host_path = if guest_path_str.starts_with('/') {
                // Absolute path: LIND_ROOT + guest_path (guest_path already has leading /)
                format!("{}{}", LIND_ROOT, guest_path_str)
            } else if !guest_path_str.is_empty() {
                // Relative path: LIND_ROOT + "/" + guest_path
                format!("{}/{}", LIND_ROOT, guest_path_str)
            } else {
                // Empty path (abstract socket): just use LIND_ROOT
                LIND_ROOT.to_string()
            };

            let new_path_len = host_path.len();
            let new_path_bytes = host_path.as_bytes();

            // Only rewrite in place if the prefixed path still fits into the 108-byte sun_path.
            if new_path_len < 108 {
                // Write the host path at the start
                ptr::copy_nonoverlapping(
                    new_path_bytes.as_ptr(),
                    sun_path_ptr as *mut u8,
                    new_path_len,
                );
                // Zero-fill the remaining tail
                ptr::write_bytes(sun_path_ptr.add(new_path_len), 0, 108 - new_path_len);

                // Keep our local mirror in sync for length calculation
                for (i, &b) in new_path_bytes.iter().enumerate() {
                    saddr.sun_path[i] = b as i8;
                }
                for b in &mut saddr.sun_path[new_path_len..] {
                    *b = 0;
                }
            }

            // Ensure the family field at the head of the original buffer is consistent.
            ptr::write_unaligned(arg as *mut u16, saddr.sun_family);
        }

        out_len = unsafe { unix_len_from_sun_path(&saddr.sun_path) };
    } else {
        // Non-UNIX families: we don’t modify the buffer; length is the canonical sizeof(*).
        out_len = match saddr.sun_family as i32 {
            libc::AF_INET => size_of::<libc::sockaddr_in>() as libc::socklen_t,
            libc::AF_INET6 => size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            _ => size_of::<libc::sockaddr>() as libc::socklen_t,
        };
    }

    (arg as *mut libc::sockaddr, out_len)
}

/// `copy_out_sockaddr` copies a sockaddr structure into a user-provided buffer,
/// adjusting the length field appropriately.  
///
/// It checks the requested address family (AF_INET/AF_INET6/AF_UNIX) and copies it into the destination buffer up to
/// the caller-provided length (`*addrlen`).  
/// If the actual sockaddr length is larger than the provided length, the data
/// is truncated; otherwise, the buffer is fully populated.  
/// The function updates `*addrlen` to reflect the actual length written or the
/// expected length in compliance with Linux socket API semantics.
///
/// This function is used to update sockaddr info after kernel syscalls (ie: accept)
pub unsafe fn copy_out_sockaddr(
    dst_user: *mut SockAddr,        // User buffer points to SockAddr
    dst_len_ptr: *mut socklen_t,    // actual length
    src_storage: &sockaddr_storage, // source addr (libc::sockaddr)
) {
    if dst_user.is_null() || dst_len_ptr.is_null() {
        return;
    }

    // Read family
    let sa_ptr = src_storage as *const _ as *const sockaddr;
    let family: sa_family_t = (*sa_ptr).sa_family;

    // Compute the "actual address length"
    let actual_len: socklen_t = match family as i32 {
        AF_INET => size_of::<sockaddr_in>() as socklen_t,
        AF_INET6 => size_of::<sockaddr_in6>() as socklen_t,
        AF_UNIX => size_of::<sockaddr_un>() as socklen_t,
        _ => 0,
    };

    // Write family into the custom SockAddr
    (*dst_user).sun_family = family as u16;

    // Determine payload size (excluding sa_family_t)
    let payload_len = match family as i32 {
        AF_INET => size_of::<sockaddr_in>() - size_of::<sa_family_t>(),
        AF_INET6 => size_of::<sockaddr_in6>() - size_of::<sa_family_t>(),
        AF_UNIX => size_of::<sockaddr_un>() - size_of::<sa_family_t>(),
        _ => 0,
    };

    if payload_len > 0 {
        // Clamp to the capacity of sun_path to avoid overflow
        let copy_len = core::cmp::min(payload_len, (*dst_user).sun_path.len());

        // Copy bytes after sa_family_t into our own sun_path
        ptr::copy_nonoverlapping(
            (sa_ptr as *const u8).add(size_of::<sa_family_t>()),
            (*dst_user).sun_path.as_mut_ptr() as *mut u8,
            copy_len,
        );

        // If payload is smaller than 108, zero the rest to keep determinism
        if copy_len < (*dst_user).sun_path.len() {
            ptr::write_bytes(
                (*dst_user).sun_path.as_mut_ptr().add(copy_len),
                0,
                (*dst_user).sun_path.len() - copy_len,
            );
        }
    } else {
        // Unknown family: zero the payload
        ptr::write_bytes(
            (*dst_user).sun_path.as_mut_ptr(),
            0,
            (*dst_user).sun_path.len(),
        );
    }

    // Write back the "actual length".
    // This value is independent of whether truncation occurred,
    // following Linux semantics.
    *dst_len_ptr = actual_len;
}

/// `convert_sockpair` validates and converts a raw pointer argument into a
/// mutable reference to a `SockPair` structure within the given cage context.  
///
/// Under the "secure" feature, the caller's cage ID is checked against the
/// current cage ID to prevent cross-cage violations.  
/// The function translates the user-space virtual address into a host-accessible
/// pointer using the cage's vmmap, then safely dereferences it into a mutable
/// reference.  
///
/// On success, it returns `Ok(&mut SockPair)`. On failure (e.g., invalid pointer
/// or unmapped memory), it returns an `EFAULT` syscall error.
pub fn convert_sockpair<'a>(
    arg: u64,
    arg_cageid: u64,
    cageid: u64,
) -> Result<&'a mut SockPair, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let pointer = arg as *mut SockPair;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}
