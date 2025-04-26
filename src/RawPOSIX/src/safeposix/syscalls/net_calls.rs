#![allow(dead_code)]
use fdtables;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::fs_const::LIND_ROOT;
use sysdefs::constants::{net_const, sys_const};
use sysdefs::data::fs_struct::*;
use sysdefs::data::net_struct::*;

use crate::interface;
use crate::interface::*;
use crate::safeposix::cage::*;
use crate::safeposix::filesystem::normpath;

use bit_set::BitSet;
use dashmap::mapref::entry;
use lazy_static::lazy_static;
use libc::*;
use libc::*;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::ffi::CStr;
use std::ffi::CString;
use std::io;
use std::io::{Read, Write};
use std::mem::size_of;
use std::sync::Arc;
use std::{os::fd::RawFd, ptr};

const FDKIND_KERNEL: u32 = 0;
const FDKIND_IMPIPE: u32 = 1;
const FDKIND_IMSOCK: u32 = 2;

lazy_static! {
    // A hashmap used to store epoll mapping relationships
    // <virtual_epfd <kernel_fd, virtual_fd>>
    static ref REAL_EPOLL_MAP: Mutex<HashMap<u64, HashMap<i32, u64>>> = Mutex::new(HashMap::new());
}

impl Cage {
    /*
     *   Mapping a new virtual fd and kernel fd that libc::socket returned
     *   Then return virtual fd
     */
    pub fn socket_syscall(&self, domain: i32, socktype: i32, protocol: i32) -> i32 {
        let kernel_fd = unsafe { libc::socket(domain, socktype, protocol) };
        /*
            get_unused_virtual_fd(cageid,realfd,is_cloexec,optionalinfo) -> Result<virtualfd, EMFILE>
        */
        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "socket");
        }

        return fdtables::get_unused_virtual_fd(
            self.cageid,
            FDKIND_KERNEL,
            kernel_fd as u64,
            false,
            0,
        )
        .unwrap() as i32;
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   bind() will return 0 when success and -1 when fail
     */
    pub fn bind_syscall(&self, virtual_fd: i32, addr: &GenSockaddr) -> i32 {
        println!("bind {} to {:?}", virtual_fd, addr);
        /*
            translate_virtual_fd(cageid: u64, virtualfd: u64) -> Result<u64, threei::RetVal>
        */
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "bind", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let mut new_addr = SockaddrUnix::default();

        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => (
                (addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>(),
            ),
            GenSockaddr::V4(addrref) => (
                (addrref as *const SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<libc::sockaddr_in>(),
            ),
            GenSockaddr::Unix(addrrefu) => {
                // Convert sun_path to LIND_ROOT path
                let original_path = unsafe {
                    CStr::from_ptr(addrrefu.sun_path.as_ptr() as *const i8)
                        .to_str()
                        .unwrap()
                };
                let lind_path = format!("{}{}", LIND_ROOT, &original_path[..]); // Skip the initial '/' in original path

                // Ensure the length of lind_path does not exceed sun_path capacity
                if lind_path.len() >= addrrefu.sun_path.len() {
                    panic!("New path is too long to fit in sun_path");
                }

                new_addr = SockaddrUnix {
                    sun_family: addrrefu.sun_family,
                    sun_path: [0; 108],
                };

                // Copy the new path into sun_path
                unsafe {
                    ptr::copy_nonoverlapping(
                        lind_path.as_ptr(),
                        new_addr.sun_path.as_mut_ptr() as *mut u8,
                        lind_path.len(),
                    );
                    *new_addr.sun_path.get_unchecked_mut(lind_path.len()) = 0; // Null-terminate the string
                }

                (
                    (&new_addr as *const SockaddrUnix).cast::<libc::sockaddr>(),
                    size_of::<SockaddrUnix>(),
                )
            }
        };

        let tmp = libc::sockaddr_in {
            sin_family: 2,
            sin_port: htons(8080),
            sin_zero: [0; 8],
            sin_addr: in_addr { s_addr: 0 },
        };

        println!("bind on real fd: {}", vfd.underfd);
        // let ret = unsafe { libc::bind(vfd.underfd as i32, (&tmp as *const libc::sockaddr_in) as *const libc::sockaddr, addrlen as u32) };
        let ret = unsafe { libc::bind(vfd.underfd as i32, finalsockaddr, addrlen as u32) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "bind");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   connect() will return 0 when success and -1 when fail
     */
    pub fn connect_syscall(&self, virtual_fd: i32, addr: &GenSockaddr) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "connect", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let mut new_addr = SockaddrUnix::default();

        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => (
                (addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>(),
            ),
            GenSockaddr::V4(addrref) => (
                (addrref as *const SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>(),
            ),
            GenSockaddr::Unix(addrrefu) => {
                // Convert sun_path to LIND_ROOT path
                let original_path = unsafe {
                    CStr::from_ptr(addrrefu.sun_path.as_ptr() as *const i8)
                        .to_str()
                        .unwrap()
                };
                let lind_path = format!("{}{}", LIND_ROOT, &original_path[..]); // Skip the initial '/' in original path

                // Ensure the length of lind_path does not exceed sun_path capacity
                if lind_path.len() >= addrrefu.sun_path.len() {
                    panic!("New path is too long to fit in sun_path");
                }

                new_addr = SockaddrUnix {
                    sun_family: addrrefu.sun_family,
                    sun_path: [0; 108],
                };

                // Copy the new path into sun_path
                unsafe {
                    ptr::copy_nonoverlapping(
                        lind_path.as_ptr(),
                        new_addr.sun_path.as_mut_ptr() as *mut u8,
                        lind_path.len(),
                    );
                    *new_addr.sun_path.get_unchecked_mut(lind_path.len()) = 0; // Null-terminate the string
                }

                (
                    (&new_addr as *const SockaddrUnix).cast::<libc::sockaddr>(),
                    size_of::<SockaddrUnix>(),
                )
            }
        };

        let ret = unsafe { libc::connect(vfd.underfd as i32, finalsockaddr, addrlen as u32) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "connect");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   sendto() will return the number of bytes sent, and -1 when fail
     */
    pub fn sendto_syscall(
        &self,
        virtual_fd: i32,
        buf: *const u8,
        buflen: usize,
        flags: i32,
        dest_addr: &GenSockaddr,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "sendto", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let (finalsockaddr, addrlen) = match dest_addr {
            GenSockaddr::V6(addrref6) => (
                (addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>(),
            ),
            GenSockaddr::V4(addrref) => (
                (addrref as *const SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>(),
            ),
            GenSockaddr::Unix(addrrefu) => {
                // Convert sun_path to LIND_ROOT path
                let original_path = unsafe {
                    CStr::from_ptr(addrrefu.sun_path.as_ptr() as *const i8)
                        .to_str()
                        .unwrap()
                };
                let lind_path = format!("{}{}", LIND_ROOT, &original_path[..]); // Skip the initial '/' in original path

                // Ensure the length of lind_path does not exceed sun_path capacity
                if lind_path.len() >= addrrefu.sun_path.len() {
                    panic!("New path is too long to fit in sun_path");
                }

                let mut new_addr = SockaddrUnix {
                    sun_family: addrrefu.sun_family,
                    sun_path: [0; 108],
                };

                // Copy the new path into sun_path
                unsafe {
                    ptr::copy_nonoverlapping(
                        lind_path.as_ptr(),
                        new_addr.sun_path.as_mut_ptr() as *mut u8,
                        lind_path.len(),
                    );
                    *new_addr.sun_path.get_unchecked_mut(lind_path.len()) = 0; // Null-terminate the string
                }

                (
                    (&new_addr as *const SockaddrUnix).cast::<libc::sockaddr>(),
                    size_of::<SockaddrUnix>(),
                )
            }
        };

        let ret = unsafe {
            libc::sendto(
                vfd.underfd as i32,
                buf as *const c_void,
                buflen,
                flags,
                finalsockaddr,
                addrlen as u32,
            ) as i32
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "sendto");
        }

        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   send() will return the number of bytes sent, and -1 when fail
     */
    pub fn send_syscall(&self, virtual_fd: i32, buf: *const u8, buflen: usize, flags: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "send", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let ret =
            unsafe { libc::send(vfd.underfd as i32, buf as *const c_void, buflen, flags) as i32 };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "send");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   recvfrom() will return
     *       - Success: the length of the message in bytes
     *       - No messages are available to be received and the
     *           peer has performed an orderly shutdown: 0
     *       - Fail: -1
     */
    pub fn recvfrom_syscall(
        &self,
        virtual_fd: i32,
        buf: *mut u8,
        buflen: usize,
        flags: i32,
        addr: &mut Option<&mut GenSockaddr>,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "recvfrom", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let (finalsockaddr, mut addrlen) = match addr {
            Some(GenSockaddr::V6(ref mut addrref6)) => (
                (addrref6 as *mut SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>() as u32,
            ),
            Some(GenSockaddr::V4(ref mut addrref)) => (
                (addrref as *mut SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>() as u32,
            ),
            Some(GenSockaddr::Unix(ref mut addrrefu)) => (
                (addrrefu as *mut SockaddrUnix).cast::<libc::sockaddr>(),
                size_of::<SockaddrUnix>() as u32,
            ),
            None => (std::ptr::null::<libc::sockaddr>() as *mut libc::sockaddr, 0),
        };

        let ret = unsafe {
            libc::recvfrom(
                vfd.underfd as i32,
                buf as *mut c_void,
                buflen,
                flags,
                finalsockaddr,
                &mut addrlen as *mut u32,
            ) as i32
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   recv() will return
     *       - Success: the length of the message in bytes
     *       - No messages are available to be received and the
     *           peer has performed an orderly shutdown: 0
     *       - Fail: -1
     */
    pub fn recv_syscall(&self, virtual_fd: i32, buf: *mut u8, len: usize, flags: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "recv", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let ret = unsafe { libc::recv(vfd.underfd as i32, buf as *mut c_void, len, flags) as i32 };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recv");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   listen() will return 0 when success and -1 when fail
     */
    pub fn listen_syscall(&self, virtual_fd: i32, backlog: i32) -> i32 {
        println!("listen {}", virtual_fd);
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "listen", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        println!("listen on real fd: {}", vfd.underfd);
        let ret = unsafe { libc::listen(vfd.underfd as i32, backlog) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "listen");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   shutdown() will return 0 when success and -1 when fail
     */
    pub fn shutdown_syscall(&self, virtual_fd: i32, how: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "shutdown", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let ret = unsafe { libc::shutdown(vfd.underfd as i32, how) };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "bind");
        }

        ret
    }

    /*
     *   We pass a default addr to libc::accept and then fill the GenSockaddr when return to
     *   dispatcher
     *
     *   Get the kernel fd with provided virtual fd first
     *   accept() will return a file descriptor for the accepted socket
     *   Mapping a new virtual fd in this cage (virtual fd is different per cage) and kernel
     *       fd that libc::accept returned
     *   Return the virtual fd
     */
    pub fn accept_syscall(&self, virtual_fd: i32, addr: &mut Option<&mut GenSockaddr>) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "accept", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let (finalsockaddr, mut addrlen) = match addr {
            Some(GenSockaddr::V6(ref mut addrref6)) => (
                (addrref6 as *mut SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>() as u32,
            ),
            Some(GenSockaddr::V4(ref mut addrref)) => (
                (addrref as *mut SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>() as u32,
            ),
            Some(GenSockaddr::Unix(ref mut addrrefu)) => (
                (addrrefu as *mut SockaddrUnix).cast::<libc::sockaddr>(),
                size_of::<SockaddrUnix>() as u32,
            ),
            None => (std::ptr::null::<libc::sockaddr>() as *mut libc::sockaddr, 0),
        };

        let ret_kernelfd =
            unsafe { libc::accept(vfd.underfd as i32, finalsockaddr, &mut addrlen as *mut u32) };

        if ret_kernelfd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "accept");
        }

        // change the GenSockaddr type according to the sockaddr we received
        // GenSockAddr will be modified after libc::accept returns
        // So we only need to modify values in GenSockAddr, and rest of the things will be finished in dispatcher stage

        if let Some(sockaddr) = addr {
            if let GenSockaddr::Unix(ref mut sockaddr_unix) = sockaddr {
                unsafe {
                    if std::slice::from_raw_parts(
                        sockaddr_unix.sun_path.as_ptr() as *const u8,
                        LIND_ROOT.len(),
                    ) == LIND_ROOT.as_bytes()
                    {
                        // Move ptr to exclue LIND_ROOT
                        let new_path_ptr = sockaddr_unix.sun_path.as_ptr().add(LIND_ROOT.len());

                        // sun_path in RawPOSIX will always be 108
                        let new_path_len = 108 - LIND_ROOT.len();

                        let mut temp_path = vec![0u8; sockaddr_unix.sun_path.len()];

                        std::ptr::copy_nonoverlapping(
                            new_path_ptr,
                            temp_path.as_mut_ptr(),
                            new_path_len,
                        );

                        for i in 0..sockaddr_unix.sun_path.len() {
                            sockaddr_unix.sun_path[i] = 0;
                        }

                        std::ptr::copy_nonoverlapping(
                            temp_path.as_ptr(),
                            sockaddr_unix.sun_path.as_mut_ptr(),
                            new_path_len,
                        );
                    }
                }
            }
        }

        let ret_virtualfd = fdtables::get_unused_virtual_fd(
            self.cageid,
            FDKIND_KERNEL,
            ret_kernelfd as u64,
            false,
            0,
        )
        .unwrap();

        ret_virtualfd as i32
    }

    /*
     *   The design logic for select is first to categorize the file descriptors (fds) received from the user based on FDKIND.
     *   Specifically, kernel fds are passed to the underlying libc select, while impipe and imsock fds would be processed by the
     *   in-memory system. Afterward, the results are combined and consolidated accordingly.
     *
     *   (Note: Currently, only kernel fds are supported. The implementation for in-memory pipes is commented out and will require
     *   further integration and testing once in-memory pipe support is added.)
     *
     *   select() will return:
     *       - the total number of bits that are set in readfds, writefds, errorfds
     *       - 0, if the timeout expired before any file descriptors became ready
     *       - -1, fail
     */
    pub fn select_syscall(
        &self,
        nfds: i32,
        mut readfds: Option<&mut fd_set>,
        mut writefds: Option<&mut fd_set>,
        mut errorfds: Option<&mut fd_set>,
        rposix_timeout: Option<RustDuration>,
    ) -> i32 {
        // println!("Size of fd_set: {}, DebugFdSet: {}", std::mem::size_of::<fd_set>(), std::mem::size_of::<DebugFdSet>());

        // println!("select: rposix_timeout: {:?}, nfds: {}", rposix_timeout, nfds);
        // if readfds.is_some() {
        //     let fds = readfds.unwrap();
        //     // let debug_readfds;
        //     // unsafe {
        //     //     debug_readfds = *((fds as *mut fd_set) as *mut DebugFdSet);
        //     // }
        //     // println!("debug_readfds: {:?}", debug_readfds);

        //     for fd in 0..nfds {
        //         unsafe {
        //             if FD_ISSET(fd, fds) {
        //                 print!("{} set, ", fd);
        //             } else {
        //                 print!("{} not set, ", fd);
        //             }
        //         }
        //     }
        //     println!("");
        //     readfds = Some(fds);
        // }
        // if rposix_timeout.is_none() {
        //     timeout = libc::timeval {
        //         tv_sec: 0,
        //         tv_usec: 0,
        //     };
        // } else {
        //     timeout = libc::timeval {
        //         tv_sec: rposix_timeout.unwrap().as_secs() as i64,
        //         tv_usec: rposix_timeout.unwrap().subsec_micros() as i64,
        //     };
        // }

        let orfds = readfds.as_mut().map(|fds| &mut **fds);
        let owfds = writefds.as_mut().map(|fds| &mut **fds);
        let oefds = errorfds.as_mut().map(|fds| &mut **fds);

        let mut fdkindset = HashSet::new();
        fdkindset.insert(FDKIND_KERNEL);

        let (selectbittables, unparsedtables, mappingtable) =
            fdtables::prepare_bitmasks_for_select(
                self.cageid,
                nfds as u64,
                orfds.copied(),
                owfds.copied(),
                oefds.copied(),
                &fdkindset,
            )
            .unwrap();
        // println!("unparsedtables: {:?}, mappingtable: {:?}", unparsedtables, mappingtable);

        // ------ libc select() ------
        // In select, each fd_set is allowed to contain empty values, as it’s possible for the user to input a mixture of pure
        // virtual_fds and those with underlying real file descriptors. This means we need to check each fd_set separately to
        // handle both types of descriptors properly. The goal here is to ensure that each fd_set (read, write, error) is correctly
        // initialized. To handle cases where selectbittables does not contain an entry at the expected index or where it doesn’t
        // include a FDKIND_KERNEL entry, the code assigns a default value with an initialized fd_set and an nfd of 0.
        let (readnfd, mut real_readfds) = selectbittables
            .get(0)
            .and_then(|table| table.get(&FDKIND_KERNEL).cloned())
            .unwrap_or((0, fdtables::_init_fd_set()));
        let (writenfd, mut real_writefds) = selectbittables
            .get(1)
            .and_then(|table| table.get(&FDKIND_KERNEL).cloned())
            .unwrap_or((0, fdtables::_init_fd_set()));
        let (errornfd, mut real_errorfds) = selectbittables
            .get(2)
            .and_then(|table| table.get(&FDKIND_KERNEL).cloned())
            .unwrap_or((0, fdtables::_init_fd_set()));

        let mut realnewnfds = readnfd.max(writenfd).max(errornfd);

        // print!("select real readfds: ");
        // for fd in 0..realnewnfds {
        //     unsafe {
        //         if FD_ISSET(fd as i32, &real_readfds) {
        //             print!("{} set, ", fd);
        //         } else {
        //             print!("{} not set, ", fd);
        //         }
        //     }
        // }
        // println!("");

        let start_time = interface::starttimer();
        let (duration, mut timeout) = interface::timeout_setup(rposix_timeout);
        let mut ret;
        loop {
            let mut tmp_readfds = real_readfds.clone();
            let mut tmp_writefds = real_writefds.clone();
            let mut tmp_errorfds = real_errorfds.clone();
            // Ensured that null_mut is used if the Option is None for fd_set parameters.
            ret = unsafe {
                libc::select(
                    realnewnfds as i32,
                    &mut tmp_readfds as *mut _,
                    &mut tmp_writefds as *mut _,
                    &mut tmp_errorfds as *mut _,
                    &mut timeout as *mut timeval,
                )
            };

            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "select");
            }

            // check for timeout
            if ret > 0 || interface::readtimer(start_time) > duration {
                real_readfds = tmp_readfds;
                real_writefds = tmp_writefds;
                real_errorfds = tmp_errorfds;
                break;
            }

            // check for signals
            if signal_check_trigger(self.cageid) {
                return syscall_error(Errno::EINTR, "select", "interrupted");
            }
        }

        let mut unreal_read = HashSet::new();
        let mut unreal_write = HashSet::new();

        // Revert result
        let (read_flags, read_result) = fdtables::get_one_virtual_bitmask_from_select_result(
            FDKIND_KERNEL,
            realnewnfds as u64,
            Some(real_readfds),
            unreal_read,
            None,
            &mappingtable,
        );

        if let Some(readfds) = readfds.as_mut() {
            **readfds = read_result.unwrap();
        }

        let (write_flags, write_result) = fdtables::get_one_virtual_bitmask_from_select_result(
            FDKIND_KERNEL,
            realnewnfds as u64,
            Some(real_writefds),
            unreal_write,
            None,
            &mappingtable,
        );

        if let Some(writefds) = writefds.as_mut() {
            **writefds = write_result.unwrap();
        }

        let (error_flags, error_result) = fdtables::get_one_virtual_bitmask_from_select_result(
            FDKIND_KERNEL,
            realnewnfds as u64,
            Some(real_errorfds),
            HashSet::new(), // Assuming there are no unreal errorsets
            None,
            &mappingtable,
        );

        if let Some(errorfds) = errorfds.as_mut() {
            **errorfds = error_result.unwrap();
        }

        // The total number of descriptors ready
        (read_flags + write_flags + error_flags) as i32
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   getsockopt() will return 0 when success and -1 when fail
     */
    pub fn getsockopt_syscall(
        &self,
        virtual_fd: i32,
        level: i32,
        optname: i32,
        optval: &mut i32,
        optlen: u32,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "getsockopt", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let mut optlen: socklen_t = 4;

        let ret = unsafe {
            libc::getsockopt(
                vfd.underfd as i32,
                level,
                optname,
                optval as *mut c_int as *mut c_void,
                &mut optlen as *mut socklen_t,
            )
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "getsockopt");
        }

        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   setsockopt() will return 0 when success and -1 when fail
     */
    pub fn setsockopt_syscall(
        &self,
        virtual_fd: i32,
        level: i32,
        optname: i32,
        optval: *mut u8,
        optlen: u32,
    ) -> i32 {
        // println!("setsockopt_syscall: fd: {}, level: {}, optname: {}, optlen: {}", virtual_fd, level, optname, optlen);
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "setsockopt", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let ret = unsafe {
            libc::setsockopt(
                vfd.underfd as i32,
                level,
                optname,
                optval as *mut c_void,
                optlen,
            )
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "setsockopt");
        }
        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   getpeername() will return 0 when success and -1 when fail
     */
    pub fn getpeername_syscall(
        &self,
        virtual_fd: i32,
        address: &mut Option<&mut GenSockaddr>,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "getpeername", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let (finalsockaddr, mut addrlen) = match address {
            Some(GenSockaddr::V6(ref mut addrref6)) => (
                (addrref6 as *mut SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>() as u32,
            ),
            Some(GenSockaddr::V4(ref mut addrref)) => (
                (addrref as *mut SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>() as u32,
            ),
            Some(GenSockaddr::Unix(ref mut addrrefu)) => (
                (addrrefu as *mut SockaddrUnix).cast::<libc::sockaddr>(),
                size_of::<SockaddrUnix>() as u32,
            ),
            None => (std::ptr::null::<libc::sockaddr>() as *mut libc::sockaddr, 0),
        };

        let ret = unsafe {
            libc::getpeername(vfd.underfd as i32, finalsockaddr, &mut addrlen as *mut u32)
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "getpeername");
        }

        if let Some(sockaddr) = address {
            if let GenSockaddr::Unix(ref mut sockaddr_unix) = sockaddr {
                unsafe {
                    if std::slice::from_raw_parts(
                        sockaddr_unix.sun_path.as_ptr() as *const u8,
                        LIND_ROOT.len(),
                    ) == LIND_ROOT.as_bytes()
                    {
                        // Move ptr to exclue LIND_ROOT
                        let new_path_ptr = sockaddr_unix.sun_path.as_ptr().add(LIND_ROOT.len());

                        // sun_path in RawPOSIX will always be 108
                        let new_path_len = 108 - LIND_ROOT.len();

                        let mut temp_path = vec![0u8; sockaddr_unix.sun_path.len()];

                        std::ptr::copy_nonoverlapping(
                            new_path_ptr,
                            temp_path.as_mut_ptr(),
                            new_path_len,
                        );

                        for i in 0..sockaddr_unix.sun_path.len() {
                            sockaddr_unix.sun_path[i] = 0;
                        }

                        std::ptr::copy_nonoverlapping(
                            temp_path.as_ptr(),
                            sockaddr_unix.sun_path.as_mut_ptr(),
                            new_path_len,
                        );
                    }
                }
            }
        }

        ret
    }

    /*
     *   Get the kernel fd with provided virtual fd first
     *   getsockname() will return 0 when success and -1 when fail
     */
    pub fn getsockname_syscall(
        &self,
        virtual_fd: i32,
        address: &mut Option<&mut GenSockaddr>,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "getsockname", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        let (finalsockaddr, mut _addrlen) = match address {
            Some(GenSockaddr::V6(ref mut addrref6)) => (
                (addrref6 as *mut SockaddrV6).cast::<libc::sockaddr>(),
                size_of::<SockaddrV6>() as u32,
            ),
            Some(GenSockaddr::V4(ref mut addrref)) => (
                (addrref as *mut SockaddrV4).cast::<libc::sockaddr>(),
                size_of::<SockaddrV4>() as u32,
            ),
            Some(GenSockaddr::Unix(ref mut addrrefu)) => (
                (addrrefu as *mut SockaddrUnix).cast::<libc::sockaddr>(),
                size_of::<SockaddrUnix>() as u32,
            ),
            None => (std::ptr::null::<libc::sockaddr>() as *mut libc::sockaddr, 0),
        };

        let mut testlen = 128 as u32;
        let ret = unsafe {
            libc::getsockname(vfd.underfd as i32, finalsockaddr, &mut testlen as *mut u32)
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "getsockname");
        }

        ret
    }

    /*
     *   gethostname() will return 0 when success and -1 when fail
     */
    pub fn gethostname_syscall(&self, name: *mut u8, len: isize) -> i32 {
        let ret = unsafe { libc::gethostname(name as *mut i8, len as usize) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "gethostname");
        }
        ret
    }

    /*
     *   In Linux, there is a specific structure pollfd used to pass file descriptors and their
     *   related event information. Through the poll() function, multiple file descriptors can be
     *   monitored at the same time, and different event monitoring can be set for each file
     *   descriptor. We implement our PollStruct and related helper functions to do translation
     *   between virtual fd and kernel fd, in order to use kernel system calls. The ownership of
     *   poll_fd should be moved once the functions returns.
     *
     *   poll() will return:
     *   - a nonnegative value which is the number of elements in the pollfds whose revents
     *   fields have been set to a nonzero value (indicating an event or an error)
     *   - the system call timed out before any file descriptors became ready
     *   - -1, fail
     */
    pub fn poll_syscall(
        &self,
        virtual_fds: &mut [PollStruct], // lots of fds, a ptr
        _nfds: u64,
        timeout: i32,
    ) -> i32 {
        let mut virfdvec = HashSet::new();

        for vpoll in &mut *virtual_fds {
            let vfd = vpoll.fd as u64;
            virfdvec.insert(vfd);
        }

        let (allhashmap, _mappingtable) =
            fdtables::convert_virtualfds_for_poll(self.cageid, virfdvec);

        let mut libc_nfds = 0;
        let mut libc_pollfds: Vec<pollfd> = Vec::new();
        for (fd_kind, fdtuple) in allhashmap {
            match fd_kind {
                FDKIND_KERNEL => {
                    for (virtfd, entry) in fdtuple {
                        if let Some(vpollstruct) =
                            virtual_fds.iter().find(|&ps| ps.fd == virtfd as i32)
                        {
                            // Convert PollStruct to libc::pollfd
                            let mut libcpollstruct = self.convert_to_libc_pollfd(vpollstruct);
                            libcpollstruct.fd = entry.underfd as i32;
                            libc_pollfds.push(libcpollstruct);
                            libc_nfds = libc_nfds + 1;
                        }
                    }
                    if libc_nfds != 0 {
                        let start_time = interface::starttimer();
                        let (duration, timeout) = interface::timeout_setup_ms(timeout);

                        let mut ret;
                        loop {
                            ret = unsafe {
                                libc::poll(libc_pollfds.as_mut_ptr(), libc_nfds as u64, timeout)
                            };

                            if ret < 0 {
                                let errno = get_errno();
                                return handle_errno(errno, "poll");
                            }

                            // check for timeout
                            if ret > 0 || interface::readtimer(start_time) > duration {
                                break;
                            }

                            // check for signal
                            if signal_check_trigger(self.cageid) {
                                return syscall_error(Errno::EINTR, "poll", "interrupted");
                            }
                        }

                        // Convert back to PollStruct
                        for (i, libcpoll) in libc_pollfds.iter().enumerate() {
                            if let Some(rposix_poll) = virtual_fds.get_mut(i) {
                                rposix_poll.revents = libcpoll.revents;
                            }
                        }

                        return ret;
                    }
                }
                _ => {
                    /*TODO
                        Need to confirm the error num (we could add fdkind specific error..? eg: EFDKIND)
                    */
                    return syscall_error(Errno::EBADFD, "poll", "Invalid fdkind");
                }
            }
        }

        // TODO: Return check...?
        0
    }

    /* POLL()
     */
    fn convert_to_libc_pollfd(&self, poll_struct: &PollStruct) -> pollfd {
        pollfd {
            fd: poll_struct.fd,
            events: poll_struct.events,
            revents: poll_struct.revents,
        }
    }

    /* EPOLL
     *   In normal Linux, epoll will perform the listed behaviors
     *
     *   epoll_create:
     *   - This function creates an epfd, which is an epoll file descriptor used to manage
     *       multiple file behaviors.
     *   epoll_ctl:
     *   - This function associates the events and the file descriptors that need to be
     *       monitored with the specific epfd.
     *   epoll_wait:
     *   - This function waits for events on the epfd and returns a list of epoll_events
     *       that have been triggered.
     *
     *   Then the processing workflow in RawPOSIX is:
     *
     *   epoll_create:
     *   When epoll_create is called, we use epoll_create_helper to create a virtual epfd.
     *   Add this virtual epfd to the global mapping table.
     *
     *   epoll_ctl:
     *   (Use try_epoll_ctl to handle the association between the virtual epfd and the
     *   events with the file descriptors.) This step involves updating the global table
     *   with the appropriate mappings.
     *
     *   epoll_wait:
     *   When epoll_wait is called, you need to convert the virtual epfd to the real epfd.
     *   Call libc::epoll_wait to perform the actual wait operation on the real epfd.
     *   Convert the resulting real events back to the virtual events using the mapping in
     *   the global table.
     */

    /*
     *   Mapping a new virtual fd and kernel fd that libc::epoll_create returned
     *   Then return virtual fd
     */
    pub fn epoll_create_syscall(&self, size: i32) -> i32 {
        // Create the kernel instance
        let kernel_fd = unsafe { libc::epoll_create(size) };

        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "epoll_create");
        }

        // Get the virtual epfd
        let virtual_epfd =
            fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, kernel_fd as u64, false, 0)
                .unwrap();

        // We don't need to update mapping table at now
        // Return virtual epfd
        virtual_epfd as i32
    }

    /*
     *   Translate before calling, and updating the glocal mapping tables according to
     *   the op.
     *   epoll_ctl() will return 0 when success and -1 when fail
     */
    pub fn epoll_ctl_syscall(
        &self,
        virtual_epfd: i32,
        op: i32,
        virtual_fd: i32,
        epollevent: &mut EpollEvent,
    ) -> i32 {
        let wrappedepfd = fdtables::translate_virtual_fd(self.cageid, virtual_epfd as u64);
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() || wrappedepfd.is_err() {
            return syscall_error(Errno::EBADF, "epoll", "Bad File Descriptor");
        }

        let vepfd = wrappedepfd.unwrap();
        let vfd = wrappedvfd.unwrap();
        // EpollEvent conversion
        let event = epollevent.events;
        let mut epoll_event = epoll_event {
            events: event,
            u64: vfd.underfd as u64,
        };

        let ret = unsafe {
            libc::epoll_ctl(
                vepfd.underfd as i32,
                op,
                vfd.underfd as i32,
                &mut epoll_event,
            )
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "epoll_ctl");
        }

        // Update the virtual list -- but we only handle the non-real fd case
        //  try_epoll_ctl will directly return a real fd in libc case
        //  - maybe we could create a new mapping table to handle the mapping relationship..?
        //      ceate inside the fdtable interface? or we could handle inside rawposix..?

        // Update the mapping table for epoll
        if op == libc::EPOLL_CTL_DEL {
            let mut epollmapping = REAL_EPOLL_MAP.lock();
            if let Some(fdmap) = epollmapping.get_mut(&(vepfd.underfd as u64)) {
                if fdmap.remove(&(vfd.underfd as i32)).is_some() {
                    if fdmap.is_empty() {
                        epollmapping.remove(&(vepfd.underfd as u64));
                    }
                    return ret;
                }
            }
        } else {
            let mut epollmapping = REAL_EPOLL_MAP.lock();
            epollmapping
                .entry(vepfd.underfd as u64)
                .or_insert_with(HashMap::new)
                .insert(vfd.underfd as i32, virtual_fd as u64);
            return ret;
        }

        // [TODO]
        // should be op not support
        -1
    }

    /*
     *   Get the kernel fd with provided virtual fd first, and then convert back to virtual
     *   epoll_wait() will return:
     *       1. the number of file descriptors ready for the requested I/O
     *       2. 0, if none
     *       3. -1, fail
     */
    pub fn epoll_wait_syscall(
        &self,
        virtual_epfd: i32,
        events: &mut [EpollEvent],
        maxevents: i32,
        timeout: i32,
    ) -> i32 {
        let wrappedepfd = fdtables::translate_virtual_fd(self.cageid, virtual_epfd as u64);
        if wrappedepfd.is_err() {
            return syscall_error(Errno::EBADF, "epoll_wait", "Bad File Descriptor");
        }
        let vepfd = wrappedepfd.unwrap();

        let mut kernel_events: Vec<epoll_event> = Vec::with_capacity(maxevents as usize);

        // Should always be null value before we call libc::epoll_wait
        kernel_events.push(epoll_event { events: 0, u64: 0 });

        let start_time = interface::starttimer();
        let (duration, timeout) = interface::timeout_setup_ms(timeout);

        let mut ret;
        loop {
            ret = unsafe {
                libc::epoll_wait(
                    vepfd.underfd as i32,
                    kernel_events.as_mut_ptr(),
                    maxevents,
                    timeout,
                )
            };
            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "epoll_wait");
            }

            // check for timeout
            if ret > 0 || interface::readtimer(start_time) > duration {
                break;
            }

            if interface::signal_check_trigger(self.cageid) {
                return syscall_error(Errno::EINTR, "epoll_wait", "interrupted");
            }
        }

        // Convert back to rustposix's data structure
        // Loop over virtual_epollfd to find corresponding mapping relationship between kernel fd and virtual fd
        for i in 0..ret as usize {
            let ret_kernelfd = kernel_events[i].u64;
            let epollmapping = REAL_EPOLL_MAP.lock();
            let ret_virtualfd = epollmapping
                .get(&(vepfd.underfd as u64))
                .and_then(|kernel_map| kernel_map.get(&(ret_kernelfd as i32)).copied());

            events[i].fd = ret_virtualfd.unwrap() as i32;
            events[i].events = kernel_events[i].events;
        }

        ret
    }

    /*
     *   socketpair() will return 0 when success and -1 when fail
     */
    pub fn socketpair_syscall(
        &self,
        domain: i32,
        type_: i32,
        protocol: i32,
        virtual_socket_vector: &mut SockPair,
    ) -> i32 {
        let mut kernel_socket_vector: [i32; 2] = [0, 0];

        let ret =
            unsafe { libc::socketpair(domain, type_, protocol, kernel_socket_vector.as_mut_ptr()) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "sockpair");
        }

        let ksv_1 = kernel_socket_vector[0];
        let ksv_2 = kernel_socket_vector[1];
        let vsv_1 =
            fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, ksv_1 as u64, false, 0)
                .unwrap();
        let vsv_2 =
            fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, ksv_2 as u64, false, 0)
                .unwrap();
        virtual_socket_vector.sock1 = vsv_1 as i32;
        virtual_socket_vector.sock2 = vsv_2 as i32;
        println!(
            "socketpair: kernel fds: {:?}, virtual fds: {:?}",
            kernel_socket_vector, virtual_socket_vector
        );
        return 0;
    }
}
