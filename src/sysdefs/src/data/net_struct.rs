use libc::{sa_family_t, sockaddr_un, sockaddr_in, sockaddr_in6, AF_UNIX, AF_INET, AF_INET6};
use libc::sockaddr;
use std::mem;
use std::ptr;
use std::os::raw::c_char;

/// A simplified socket address structure supporting AF_UNIX, AF_INET, and AF_INET6.
/// This abstraction stores the address family and a 108-byte path or address buffer,
/// reused for all supported types.
#[repr(C)]
pub struct SockAddr {
    pub sun_family: u16,
    pub sun_path: [c_char; 108],
}

impl SockAddr {
    /// Initializes a new UNIX domain socket address.
    pub fn new_unix() -> Self {
        SockAddr {
            sun_family: AF_UNIX as u16,
            sun_path: [0; 108],
        }
    }

    /// Initializes a new IPv4 socket address placeholder.
    pub fn new_ipv4() -> Self {
        SockAddr {
            sun_family: AF_INET as u16,
            sun_path: [0; 108],
        }
    }

    /// Initializes a new IPv6 socket address placeholder.
    pub fn new_ipv6() -> Self {
        SockAddr {
            sun_family: AF_INET6 as u16,
            sun_path: [0; 108],
        }
    }

    /// Returns the expected length of the address structure 
    /// based on the current address family.
    pub fn get_len(&self) -> u32 {
        match self.sun_family as i32 {
            AF_INET => mem::size_of::<libc::sockaddr_in>() as u32,
            AF_INET6 => mem::size_of::<libc::sockaddr_in6>() as u32,
            AF_UNIX => mem::size_of::<libc::sockaddr_un>() as u32,
            _ => 0,
        }
    }

    /// Creates a `SockAddr` from a raw pointer to a `sockaddr`.
    /// This function safely copies the address content based on its family,
    /// skipping the sa_family_t field and storing the rest into `sun_path`.
    pub fn clone_to_sockaddr(addr: *mut u8) -> Self {
        let mut out = SockAddr {
            sun_family: 0,
            sun_path: [0; 108],
        };

        if addr.is_null() {
            return out;
        }

        unsafe {
            let addr = addr as *const sockaddr;
            let family = (*addr).sa_family;
            out.sun_family = family;

            let copy_len = match family as i32 {
                AF_UNIX => size_of::<sockaddr_un>() - size_of::<sa_family_t>(),
                AF_INET => size_of::<sockaddr_in>() - size_of::<sa_family_t>(),
                AF_INET6 => size_of::<sockaddr_in6>() - size_of::<sa_family_t>(),
                _ => 0,
            };

            let safe_len = std::cmp::min(copy_len, 108);

            ptr::copy_nonoverlapping(
                (addr as *const u8).add(size_of::<sa_family_t>()),
                out.sun_path.as_mut_ptr() as *mut u8,
                safe_len,
            );
        }

        out
    }
}

#[repr(C)]
pub struct SockPair {
    pub sock1: i32,
    pub sock2: i32,
}
