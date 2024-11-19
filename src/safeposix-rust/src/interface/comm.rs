// // Authors: Nicholas Renner and Jonathan Singer and Tristan Brigham
// //
// //

use crate::interface;
use std::fs::read_to_string;
use std::mem::size_of;
use std::str::from_utf8;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

extern crate libc;

static NET_DEV_FILENAME: &str = "net_devices";

static mut UD_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum GenSockaddr {
    Unix(SockaddrUnix),
    V4(SockaddrV4),
    V6(SockaddrV6),
}
impl GenSockaddr {
    pub fn port(&self) -> u16 {
        match self {
            GenSockaddr::Unix(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V4(v4addr) => v4addr.sin_port,
            GenSockaddr::V6(v6addr) => v6addr.sin6_port,
        }
    }
    pub fn set_port(&mut self, port: u16) {
        match self {
            GenSockaddr::Unix(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V4(v4addr) => v4addr.sin_port = port,
            GenSockaddr::V6(v6addr) => v6addr.sin6_port = port,
        };
    }

    pub fn addr(&self) -> GenIpaddr {
        match self {
            GenSockaddr::Unix(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V4(v4addr) => GenIpaddr::V4(v4addr.sin_addr),
            GenSockaddr::V6(v6addr) => GenIpaddr::V6(v6addr.sin6_addr),
        }
    }

    pub fn set_addr(&mut self, ip: GenIpaddr) {
        match self {
            GenSockaddr::Unix(_unixaddr) => {
                panic!("Invalid function called for this type of Sockaddr.")
            }
            GenSockaddr::V4(v4addr) => {
                v4addr.sin_addr = if let GenIpaddr::V4(v4ip) = ip {
                    v4ip
                } else {
                    unreachable!()
                }
            }
            GenSockaddr::V6(v6addr) => {
                v6addr.sin6_addr = if let GenIpaddr::V6(v6ip) = ip {
                    v6ip
                } else {
                    unreachable!()
                }
            }
        };
    }

    pub fn set_family(&mut self, family: u16) {
        match self {
            GenSockaddr::Unix(unixaddr) => unixaddr.sun_family = family,
            GenSockaddr::V4(v4addr) => v4addr.sin_family = family,
            GenSockaddr::V6(v6addr) => v6addr.sin6_family = family,
        };
    }

    pub fn get_family(&self) -> u16 {
        match self {
            GenSockaddr::Unix(unixaddr) => unixaddr.sun_family,
            GenSockaddr::V4(v4addr) => v4addr.sin_family,
            GenSockaddr::V6(v6addr) => v6addr.sin6_family,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            GenSockaddr::Unix(unixaddr) => {
                let pathiter = &mut unixaddr.sun_path.split(|idx| *idx == 0);
                let pathslice = pathiter.next().unwrap();
                let path = from_utf8(pathslice).unwrap();
                path
            }
            GenSockaddr::V4(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V6(_) => panic!("Invalid function called for this type of Sockaddr."),
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum GenIpaddr {
    V4(V4Addr),
    V6(V6Addr),
}

impl GenIpaddr {
    pub fn is_unspecified(&self) -> bool {
        match self {
            GenIpaddr::V4(v4ip) => v4ip.s_addr == 0,
            GenIpaddr::V6(v6ip) => v6ip.s6_addr == [0; 16],
        }
    }
    pub fn from_string(string: &str) -> Option<Self> {
        let v4candidate: Vec<&str> = string.split('.').collect();
        let v6candidate: Vec<&str> = string.split(':').collect();
        let v4l = v4candidate.len();
        let v6l = v6candidate.len();
        if v4l == 1 && v6l > 1 {
            //then we should try parsing it as an ipv6 address
            let mut shortarr = [0u8; 16];
            let mut shortindex = 0;
            let mut encountered_doublecolon = false;
            for short in v6candidate {
                if short.is_empty() {
                    //you can only have a double colon once in an ipv6 address
                    if encountered_doublecolon {
                        return None;
                    }
                    encountered_doublecolon = true;

                    let numzeros = 8 - v6l + 1; //+1 to account for this empty string element
                    if numzeros == 0 {
                        return None;
                    }
                    shortindex += numzeros;
                } else {
                    //ok we can actually parse the element in this case
                    if let Ok(b) = short.parse::<u16>() {
                        //manually handle big endianness
                        shortarr[2 * shortindex] = (b >> 8) as u8;
                        shortarr[2 * shortindex + 1] = (b & 0xff) as u8;
                        shortindex += 1;
                    } else {
                        return None;
                    }
                }
            }
            return Some(Self::V6(V6Addr { s6_addr: shortarr }));
        } else if v4l == 4 && v6l == 1 {
            //then we should try parsing it as an ipv4 address
            let mut bytearr = [0u8; 4];
            let mut shortindex = 0;
            for byte in v4candidate {
                if let Ok(b) = byte.parse::<u8>() {
                    bytearr[shortindex] = b;
                    shortindex += 1;
                } else {
                    return None;
                }
            }
            return Some(Self::V4(V4Addr {
                s_addr: u32::from_ne_bytes(bytearr),
            }));
        } else {
            return None;
        }
    }
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct SockaddrUnix {
    pub sun_family: u16,
    pub sun_path: [u8; 108],
}

pub fn new_sockaddr_unix(family: u16, path: &[u8]) -> SockaddrUnix {
    let pathlen = path.len();
    if pathlen > 108 {
        panic!("Unix domain paths cannot exceed 108 bytes.")
    }
    let mut array_path: [u8; 108] = [0; 108];
    array_path[0..pathlen].copy_from_slice(path);
    SockaddrUnix {
        sun_family: family,
        sun_path: array_path,
    }
}

pub fn gen_ud_path() -> String {
    let mut owned_path: String = "/sock".to_owned();
    unsafe {
        let id = UD_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        owned_path.push_str(&id.to_string());
    }
    owned_path.clone()
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct V4Addr {
    pub s_addr: u32,
}
#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct SockaddrV4 {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: V4Addr,
    pub padding: u64,
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct V6Addr {
    pub s6_addr: [u8; 16],
}
#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct SockaddrV6 {
    pub sin6_family: u16,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: V6Addr,
    pub sin6_scope_id: u32,
}

#[derive(Debug)]
pub struct Socket {
    pub raw_sys_fd: i32,
}

impl Socket {
    pub fn new(domain: i32, socktype: i32, protocol: i32) -> Socket {
        let fd = unsafe { libc::socket(domain, socktype, protocol) };

        //we make every socket have a recieve timeout of one second
        //This is in order to allow the socket to process and recieve
        //shutdowns while blocked on blocking recv syscalls.
        let timeoutval = libc::timeval {
            tv_sec: 1,
            tv_usec: 0,
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                (&timeoutval as *const libc::timeval) as *const libc::c_void,
                size_of::<libc::timeval>() as u32,
            )
        };
        if fd < 0 {
            panic!("Socket creation failed when it should never fail");
        }
        Self { raw_sys_fd: fd }
    }

    pub fn bind(&self, addr: &GenSockaddr) -> i32 {
        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => (
                (addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>(),
            ),
            GenSockaddr::V4(addrref) => (
                (addrref as *const SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>(),
            ),
            _ => {
                unreachable!()
            }
        };
        unsafe { libc::bind(self.raw_sys_fd, finalsockaddr, addrlen as u32) }
    }

    pub fn connect(&self, addr: &GenSockaddr) -> i32 {
        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => (
                (addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>(),
            ),
            GenSockaddr::V4(addrref) => (
                (addrref as *const SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>(),
            ),
            _ => {
                unreachable!()
            }
        };
        unsafe { libc::connect(self.raw_sys_fd, finalsockaddr, addrlen as u32) }
    }

    pub fn sendto(&self, buf: *const u8, len: usize, addr: Option<&GenSockaddr>) -> i32 {
        let (finalsockaddr, addrlen) = match addr {
            Some(GenSockaddr::V6(addrref6)) => (
                (addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>(),
            ),
            Some(GenSockaddr::V4(addrref)) => (
                (addrref as *const SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>(),
            ),
            Some(_) => {
                unreachable!()
            }
            None => (
                std::ptr::null::<libc::sockaddr>() as *const libc::sockaddr,
                0,
            ),
        };
        unsafe {
            libc::sendto(
                self.raw_sys_fd,
                buf as *const libc::c_void,
                len,
                0,
                finalsockaddr,
                addrlen as u32,
            ) as i32
        }
    }

    pub fn writev(&self, iov: *const interface::IovecStruct, iovcnt: i32) -> i32 {
        unsafe { libc::writev(self.raw_sys_fd, iov as *const libc::iovec, iovcnt) as i32 }
    }

    pub fn recvfrom(&self, buf: *mut u8, len: usize, addr: &mut Option<&mut GenSockaddr>) -> i32 {
        let (finalsockaddr, mut addrlen) = match addr {
            Some(GenSockaddr::V6(ref mut addrref6)) => (
                (addrref6 as *mut SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>() as u32,
            ),
            Some(GenSockaddr::V4(ref mut addrref)) => (
                (addrref as *mut SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>() as u32,
            ),
            Some(_) => {
                unreachable!()
            }
            None => (std::ptr::null::<libc::sockaddr>() as *mut libc::sockaddr, 0),
        };
        unsafe {
            libc::recvfrom(
                self.raw_sys_fd,
                buf as *mut libc::c_void,
                len,
                0,
                finalsockaddr,
                &mut addrlen as *mut u32,
            ) as i32
        }
    }

    pub fn recvfrom_nonblocking(
        &self,
        buf: *mut u8,
        len: usize,
        addr: &mut Option<&mut GenSockaddr>,
    ) -> i32 {
        let (finalsockaddr, mut addrlen) = match addr {
            Some(GenSockaddr::V6(ref mut addrref6)) => (
                (addrref6 as *mut SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>() as u32,
            ),
            Some(GenSockaddr::V4(ref mut addrref)) => (
                (addrref as *mut SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>() as u32,
            ),
            Some(_) => {
                unreachable!()
            }
            None => (std::ptr::null::<libc::sockaddr>() as *mut libc::sockaddr, 0),
        };
        self.set_nonblocking();
        let retval = unsafe {
            libc::recvfrom(
                self.raw_sys_fd,
                buf as *mut libc::c_void,
                len,
                0,
                finalsockaddr,
                &mut addrlen as *mut u32,
            ) as i32
        };
        self.set_blocking();
        retval
    }

    pub fn listen(&self, backlog: i32) -> i32 {
        unsafe { libc::listen(self.raw_sys_fd, backlog) }
    }

    pub fn set_blocking(&self) -> i32 {
        unsafe { libc::fcntl(self.raw_sys_fd, libc::F_SETFL, 0) }
    }

    pub fn set_nonblocking(&self) -> i32 {
        unsafe { libc::fcntl(self.raw_sys_fd, libc::F_SETFL, libc::O_NONBLOCK) }
    }

    pub fn accept(&self, isv4: bool) -> (Result<Self, i32>, GenSockaddr) {
        return if isv4 {
            let mut inneraddrbuf = SockaddrV4::default();
            let mut sadlen = size_of::<SockaddrV4>() as u32;
            let newfd = unsafe {
                libc::accept(
                    self.raw_sys_fd,
                    (&mut inneraddrbuf as *mut SockaddrV4).cast::<libc::sockaddr>(),
                    &mut sadlen as *mut u32,
                )
            };

            if newfd < 0 {
                (Err(newfd), GenSockaddr::V4(inneraddrbuf))
            } else {
                (
                    Ok(Self { raw_sys_fd: newfd }),
                    GenSockaddr::V4(inneraddrbuf),
                )
            }
        } else {
            let mut inneraddrbuf = SockaddrV6::default();
            let mut sadlen = size_of::<SockaddrV6>() as u32;
            let newfd = unsafe {
                libc::accept(
                    self.raw_sys_fd,
                    (&mut inneraddrbuf as *mut SockaddrV6).cast::<libc::sockaddr>(),
                    &mut sadlen as *mut u32,
                )
            };

            if newfd < 0 {
                (Err(newfd), GenSockaddr::V6(inneraddrbuf))
            } else {
                (
                    Ok(Self { raw_sys_fd: newfd }),
                    GenSockaddr::V6(inneraddrbuf),
                )
            }
        };
    }

    pub fn nonblock_accept(&self, isv4: bool) -> (Result<Self, i32>, GenSockaddr) {
        return if isv4 {
            let mut inneraddrbuf = SockaddrV4::default();
            let mut sadlen = size_of::<SockaddrV4>() as u32;
            self.set_nonblocking();
            let newfd = unsafe {
                libc::accept(
                    self.raw_sys_fd,
                    (&mut inneraddrbuf as *mut SockaddrV4).cast::<libc::sockaddr>(),
                    &mut sadlen as *mut u32,
                )
            };
            self.set_blocking();

            if newfd < 0 {
                (Err(newfd), GenSockaddr::V4(inneraddrbuf))
            } else {
                (
                    Ok(Self { raw_sys_fd: newfd }),
                    GenSockaddr::V4(inneraddrbuf),
                )
            }
        } else {
            let mut inneraddrbuf = SockaddrV6::default();
            let mut sadlen = size_of::<SockaddrV6>() as u32;
            self.set_nonblocking();
            let newfd = unsafe {
                libc::accept(
                    self.raw_sys_fd,
                    (&mut inneraddrbuf as *mut SockaddrV6).cast::<libc::sockaddr>(),
                    &mut sadlen as *mut u32,
                )
            };
            self.set_blocking();

            if newfd < 0 {
                (Err(newfd), GenSockaddr::V6(inneraddrbuf))
            } else {
                (
                    Ok(Self { raw_sys_fd: newfd }),
                    GenSockaddr::V6(inneraddrbuf),
                )
            }
        };
    }

    pub fn setsockopt(&self, level: i32, optname: i32, optval: i32) -> i32 {
        let valbuf = optval;
        let ret = unsafe {
            libc::setsockopt(
                self.raw_sys_fd,
                level,
                optname,
                (&valbuf as *const i32).cast::<libc::c_void>(),
                size_of::<i32>() as u32,
            )
        };
        ret
    }

    pub fn shutdown(&self, how: i32) -> i32 {
        let ret = unsafe { libc::shutdown(self.raw_sys_fd, how) };
        ret
    }

    pub fn check_rawconnection(&self) -> bool {
        let mut valbuf = 0;
        let mut len = size_of::<i32>() as u32;
        let ret = unsafe {
            libc::getsockopt(
                self.raw_sys_fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut valbuf as *mut i32).cast::<libc::c_void>(),
                &mut len as *mut u32,
            )
        };
        (ret == 0) && (valbuf == 0) // if return val is 0 and error is 0 it's connected
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.raw_sys_fd);
        }
    }
}

pub fn getifaddrs_from_file() -> String {
    read_to_string(NET_DEV_FILENAME)
        .expect("No net_devices file present!")
        .to_owned()
}

// Implementations of select related FD_SET structure
pub struct FdSet(libc::fd_set);

impl FdSet {
    pub fn new() -> FdSet {
        unsafe {
            let mut raw_fd_set = std::mem::MaybeUninit::<libc::fd_set>::uninit();
            libc::FD_ZERO(raw_fd_set.as_mut_ptr());
            FdSet(raw_fd_set.assume_init())
        }
    }

    pub fn new_from_ptr(raw_fdset_ptr: *const libc::fd_set) -> &'static mut FdSet {
        unsafe { &mut *(raw_fdset_ptr as *mut FdSet) }
    }

    // copy the src FdSet into self
    pub fn copy_from(&mut self, src_fds: &FdSet) {
        unsafe {
            std::ptr::copy_nonoverlapping(
                &src_fds.0 as *const libc::fd_set,
                &mut self.0 as *mut libc::fd_set,
                1,
            );
        }
    }

    // turn off the fd bit in fd_set (currently only used by the tests)
    #[allow(dead_code)]
    pub fn clear(&mut self, fd: i32) {
        unsafe { libc::FD_CLR(fd, &mut self.0) }
    }

    // turn on the fd bit in fd_set
    pub fn set(&mut self, fd: i32) {
        unsafe { libc::FD_SET(fd, &mut self.0) }
    }

    // return true if the bit for fd is set, false otherwise
    pub fn is_set(&self, fd: i32) -> bool {
        unsafe { libc::FD_ISSET(fd, &self.0) }
    }

    pub fn is_empty(&self) -> bool {
        let fd_array: &[u8] = unsafe {
            std::slice::from_raw_parts(&self.0 as *const _ as *const u8, size_of::<libc::fd_set>())
        };
        fd_array.iter().all(|&byte| byte == 0)
    }

    // for each fd, if kernel_fds turned it on, then self will turn the corresponding tranlated fd on
    pub fn set_from_kernelfds_and_translate(
        &mut self,
        kernel_fds: &FdSet,
        nfds: i32,
        rawfd_lindfd_tuples: &Vec<(i32, i32)>,
    ) {
        for fd in 0..nfds {
            if !kernel_fds.is_set(fd) {
                continue;
            }
            // translate and set
            if let Some((_, lindfd)) = rawfd_lindfd_tuples.iter().find(|(rawfd, _)| *rawfd == fd) {
                self.set(*lindfd);
            }
        }
    }
}

// for unwrapping in kernel_select
fn to_fdset_ptr(opt: Option<&mut FdSet>) -> *mut libc::fd_set {
    match opt {
        None => std::ptr::null_mut(),
        Some(&mut FdSet(ref mut raw_fd_set)) => raw_fd_set,
    }
}

pub fn kernel_select(
    nfds: libc::c_int,
    readfds: Option<&mut FdSet>,
    writefds: Option<&mut FdSet>,
    errorfds: Option<&mut FdSet>,
) -> i32 {
    // Call libc::select and store the result
    let result = unsafe {
        // Create a timeval struct with zero timeout

        let mut kselect_timeout = libc::timeval {
            tv_sec: 0,  // 0 seconds
            tv_usec: 0, // 0 microseconds
        };

        libc::select(
            nfds,
            to_fdset_ptr(readfds),
            to_fdset_ptr(writefds),
            to_fdset_ptr(errorfds),
            &mut kselect_timeout as *mut libc::timeval,
        )
    };

    return result;
}
