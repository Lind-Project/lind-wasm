#![allow(dead_code)]
use crate::interface;
use crate::interface::errnos::{syscall_error, Errno};

use libc::*;
use std::ffi::CStr;
use std::io;
use std::io::{Read, Write};
use std::ptr::null;
use std::str::Utf8Error;

const SIZEOF_SOCKADDR: u32 = 16;

//redefining the FSData struct in this file so that we maintain flow of program
//derive eq attributes for testing whether the structs equal other fsdata structs from stat/fstat
#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct FSData {
    pub f_type: u64,
    pub f_bsize: u64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    //total files in the file system -- should be infinite
    pub f_files: u64,
    //free files in the file system -- should be infinite
    pub f_ffiles: u64,
    pub f_fsid: u64,
    //not really a limit for naming, but 254 works
    pub f_namelen: u64,
    //arbitrary val for blocksize as well
    pub f_frsize: u64,
    pub f_spare: [u8; 32],
}

//redefining the StatData struct in this file so that we maintain flow of program
//derive eq attributes for testing whether the structs equal other statdata structs from stat/fstat
#[derive(Eq, PartialEq, Default, Debug)]
#[repr(C)]
pub struct StatData {
    pub st_dev: u64,
    pub st_ino: usize,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: usize,
    pub st_blksize: i32,
    pub st_blocks: u32,
    //currently we don't populate or care about the time bits here
    pub st_atim: (u64, u64),
    pub st_mtim: (u64, u64),
    pub st_ctim: (u64, u64),
}

//R Limit for getrlimit system call
#[repr(C)]
pub struct Rlimit {
    pub rlim_cur: u64,
    pub rlim_max: u64,
}

#[derive(Eq, PartialEq, Default, Copy, Clone, Debug)]
#[repr(C)]
pub struct PipeArray {
    pub readfd: i32,
    pub writefd: i32,
}

#[derive(Eq, PartialEq, Default, Copy, Clone, Debug)]
#[repr(C)]
pub struct SockPair {
    pub sock1: i32,
    pub sock2: i32,
}

//EPOLL
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct EpollEvent {
    pub events: u32,
    pub fd: i32, //in native this is a union which could be one of a number of things
                 //however, we only support EPOLL_CTL subcommands which take the fd
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct PollStruct {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
pub struct SockaddrDummy {
    pub sa_family: u16,
    pub _sa_data: [u16; 14],
}

#[derive(Debug)]
#[repr(C)]
pub struct TimeVal {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

#[derive(Debug)]
#[repr(C)]
pub struct ITimerVal {
    pub it_interval: TimeVal,
    pub it_value: TimeVal,
}

#[repr(C)]
pub struct TimeSpec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union IoctlPtrUnion {
    pub int_ptr: *mut i32,
    pub c_char_ptr: *mut u8, //Right now, we do not support passing struct pointers to ioctl as the related call are not implemented
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct IpcPermStruct {
    pub __key: i32,
    pub uid: u32,
    pub gid: u32,
    pub cuid: u32,
    pub cgid: u32,
    pub mode: u16,
    pub __pad1: u16,
    pub __seq: u16,
    pub __pad2: u16,
    pub __unused1: u32,
    pub __unused2: u32,
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct ShmidsStruct {
    pub shm_perm: IpcPermStruct,
    pub shm_segsz: u32,
    pub shm_atime: isize,
    pub shm_dtime: isize,
    pub shm_ctime: isize,
    pub shm_cpid: u32,
    pub shm_lpid: u32,
    pub shm_nattch: u32,
}

pub type SigsetType = u64;

pub type IovecStruct = libc::iovec;

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct SigactionStruct {
    pub sa_handler: u32,
    pub sa_mask: SigsetType,
    pub sa_flags: i32,
}

use std::mem::size_of;

// Represents a Dirent struct without the string, as rust has no flexible array member support
#[repr(C, packed(1))]
pub struct ClippedDirent {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
}

pub const CLIPPED_DIRENT_SIZE: u32 = size_of::<interface::ClippedDirent>() as u32;

/*
This file provides essential functions for handling and validating `u64` inputs,
converting them to various system-specific data types needed in system calls.
It includes utilities for transforming raw pointers to typed structures, such as integer,
buffer, and string pointers, as well as complex structures like polling, signal handling,
timing, and socket-related types. Each function ensures safe and correct usage by performing
null checks, boundary validations, and type casting, returning either a valid reference
or an error if data is invalid. This design promotes secure, reliable access to memory and
 resources in a low-level systems environment.
*/
pub fn get_int(generic_argument: u64) -> Result<i32, i32> {
    let data = generic_argument as i32;
    let type_checker = (!0xffffffff) as u64;

    if (generic_argument & (!type_checker)) == 0 {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EINVAL,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_uint(generic_argument: u64) -> Result<u32, i32> {
    let data = generic_argument as u32;
    let type_checker = (!0xffffffff) as u64;

    if (generic_argument & (!type_checker)) == 0 {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EINVAL,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_long(generic_argument: u64) -> Result<i64, i32> {
    return Ok(generic_argument as i64); //this should not return error
}

pub fn get_ulong(generic_argument: u64) -> Result<u64, i32> {
    return Ok(generic_argument); //this should not return error
}

pub fn get_isize(generic_argument: u64) -> Result<isize, i32> {
    // also should not return error
    return Ok(generic_argument as isize);
}

pub fn get_usize(generic_argument: u64) -> Result<usize, i32> {
    //should not return an error
    return Ok(generic_argument as usize);
}

pub fn get_cbuf(generic_argument: u64) -> Result<*const u8, i32> {
    let data = generic_argument as *const u8;
    if !data.is_null() {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_mutcbuf(generic_argument: u64) -> Result<*mut u8, i32> {
    let data = generic_argument as *mut u8;
    if !data.is_null() {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

// for the case where the buffer pointer being Null is normal
pub fn get_mutcbuf_null(generic_argument: u64) -> Result<Option<*mut u8>, i32> {
    let data = generic_argument as *mut u8;
    if !data.is_null() {
        return Ok(Some(data));
    }
    return Ok(None);
}

pub fn get_fdset(generic_argument: u64) -> Result<Option<&'static mut fd_set>, i32> {
    let data = generic_argument as *mut libc::fd_set;
    if !data.is_null() {
        let internal_fds = unsafe { &mut *(data as *mut fd_set) };
        return Ok(Some(internal_fds));
    }
    return Ok(None);
}

pub fn get_cstr<'a>(generic_argument: u64) -> Result<&'a str, i32> {
    //first we check that the pointer is not null
    //and then we check so that we can get data from the memory

    let pointer = generic_argument as *const i8;
    if !pointer.is_null() {
        if let Ok(ret_data) = unsafe { interface::charstar_to_ruststr(pointer) } {
            return Ok(ret_data);
        } else {
            return Err(syscall_error(
                Errno::EILSEQ,
                "dispatcher",
                "could not parse input data to a string",
            ));
        }
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_cstrarr<'a>(generic_argument: u64) -> Result<Vec<&'a str>, i32> {
    //iterate though the pointers in a function and:
    //  1: check that the pointer is not null
    //  2: push the data from that pointer onto the vector being returned
    //once we encounter a null pointer, we know that we have either hit the end of the array or another null pointer in the memory

    let mut pointer = generic_argument as *const *const i8;
    let mut data_vector: Vec<&str> = Vec::new();

    if !pointer.is_null() {
        while unsafe { !(*pointer).is_null() } {
            if let Ok(character_bytes) = unsafe { interface::charstar_to_ruststr(*pointer) } {
                data_vector.push(character_bytes);
                pointer = pointer.wrapping_offset(1);
            } else {
                return Err(syscall_error(
                    Errno::EILSEQ,
                    "dispatcher",
                    "could not parse input data to string",
                ));
            }
        }
        return Ok(data_vector);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_statdatastruct<'a>(generic_argument: u64) -> Result<&'a mut StatData, i32> {
    let pointer = generic_argument as *mut StatData;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_fsdatastruct<'a>(generic_argument: u64) -> Result<&'a mut FSData, i32> {
    let pointer = generic_argument as *mut FSData;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_shmidstruct<'a>(generic_argument: u64) -> Result<&'a mut ShmidsStruct, i32> {
    let pointer = generic_argument as *mut ShmidsStruct;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_ioctlptrunion<'a>(generic_argument: u64) -> Result<&'a mut u8, i32> {
    let pointer = generic_argument as *mut u8;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_i32_ref<'a>(generic_argument: u64) -> Result<&'a mut i32, i32> {
    unsafe { Ok(&mut *((generic_argument) as *mut i32)) }
}

pub fn get_pipearray<'a>(generic_argument: u64) -> Result<&'a mut PipeArray, i32> {
    let pointer = generic_argument as *mut PipeArray;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_sockpair<'a>(generic_argument: u64) -> Result<&'a mut SockPair, i32> {
    let pointer = generic_argument as *mut SockPair;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_constsockaddr<'a>(generic_argument: u64) -> Result<&'a SockaddrDummy, i32> {
    let pointer = generic_argument as *const SockaddrDummy;
    if !pointer.is_null() {
        return Ok(unsafe { &*pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_sockaddr(generic_argument: u64, addrlen: u32) -> Result<interface::GenSockaddr, i32> {
    let pointer = generic_argument as *const SockaddrDummy;
    if !pointer.is_null() {
        let tmpsock = unsafe { &*pointer };
        match tmpsock.sa_family {
            /*AF_UNIX*/
            1 => {
                if addrlen < SIZEOF_SOCKADDR
                    || addrlen > size_of::<interface::SockaddrUnix>() as u32
                {
                    return Err(syscall_error(
                        Errno::EINVAL,
                        "dispatcher",
                        "input length incorrect for family of sockaddr",
                    ));
                }
                let unix_ptr = pointer as *const interface::SockaddrUnix;
                return Ok(interface::GenSockaddr::Unix(unsafe { *unix_ptr }));
            }
            /*AF_INET*/
            2 => {
                if addrlen < size_of::<interface::SockaddrV4>() as u32 {
                    return Err(syscall_error(
                        Errno::EINVAL,
                        "dispatcher",
                        "input length too small for family of sockaddr",
                    ));
                }
                let v4_ptr = pointer as *const interface::SockaddrV4;
                return Ok(interface::GenSockaddr::V4(unsafe { *v4_ptr }));
            }
            /*AF_INET6*/
            30 => {
                if addrlen < size_of::<interface::SockaddrV6>() as u32 {
                    return Err(syscall_error(
                        Errno::EINVAL,
                        "dispatcher",
                        "input length too small for family of sockaddr",
                    ));
                }
                let v6_ptr = pointer as *const interface::SockaddrV6;
                return Ok(interface::GenSockaddr::V6(unsafe { *v6_ptr }));
            }
            val => {
                return Err(syscall_error(
                    Errno::EOPNOTSUPP,
                    "dispatcher",
                    "sockaddr family not supported",
                ))
            }
        }
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn set_gensockaddr(
    generic_argument: u64,
    generic_argument1: u64,
) -> Result<interface::GenSockaddr, i32> {
    let received = generic_argument as *mut SockaddrDummy;
    let received_addrlen = (generic_argument1 as *mut u32) as u32;
    let tmpsock = unsafe { &*received };
    match tmpsock.sa_family {
        /*AF_UNIX*/
        1 => {
            if received_addrlen < SIZEOF_SOCKADDR
                || received_addrlen > size_of::<interface::SockaddrUnix>() as u32
            {
                return Err(syscall_error(
                    Errno::EINVAL,
                    "dispatcher",
                    "input length incorrect for family of sockaddr",
                ));
            }
            let unix_addr = interface::GenSockaddr::Unix(interface::SockaddrUnix::default());
            return Ok(unix_addr);
        }
        /*AF_INET*/
        2 => {
            if received_addrlen < size_of::<interface::SockaddrV4>() as u32 {
                return Err(syscall_error(
                    Errno::EINVAL,
                    "dispatcher",
                    "input length too small for family of sockaddr",
                ));
            }
            let v4_addr = interface::GenSockaddr::V4(interface::SockaddrV4::default());
            return Ok(v4_addr);
        }
        /*AF_INET6*/
        30 => {
            if received_addrlen < size_of::<interface::SockaddrV6>() as u32 {
                return Err(syscall_error(
                    Errno::EINVAL,
                    "dispatcher",
                    "input length too small for family of sockaddr",
                ));
            }
            let v6_addr = interface::GenSockaddr::V6(interface::SockaddrV6::default());
            return Ok(v6_addr);
        }
        _ => {
            let null_addr = interface::GenSockaddr::Unix(interface::SockaddrUnix::default());
            return Ok(null_addr);
        }
    }
}

pub fn copy_out_sockaddr(
    generic_argument: u64,
    generic_argument1: u64,
    gensock: interface::GenSockaddr,
) {
    let copyoutaddr = (generic_argument as *mut SockaddrDummy) as *mut u8;
    let addrlen = generic_argument1 as *mut u32;
    assert!(!copyoutaddr.is_null());
    assert!(!addrlen.is_null());
    let initaddrlen = unsafe { *addrlen };
    let mut mutgensock = gensock;
    match mutgensock {
        interface::GenSockaddr::Unix(ref mut unixa) => {
            let unixlen = size_of::<interface::SockaddrUnix>() as u32;

            let fullcopylen = interface::rust_min(initaddrlen, unixlen);
            unsafe {
                std::ptr::copy(
                    (unixa) as *mut interface::SockaddrUnix as *mut u8,
                    copyoutaddr,
                    initaddrlen as usize,
                )
            };
            unsafe {
                *addrlen = interface::rust_max(unixlen, fullcopylen);
            }
        }

        interface::GenSockaddr::V4(ref mut v4a) => {
            let v4len = size_of::<interface::SockaddrV4>() as u32;

            let fullcopylen = interface::rust_min(initaddrlen, v4len);

            unsafe {
                std::ptr::copy(
                    (v4a) as *mut interface::SockaddrV4 as *mut u8,
                    copyoutaddr,
                    initaddrlen as usize,
                )
            };
            unsafe {
                *addrlen = interface::rust_max(v4len, fullcopylen);
            }
        }

        interface::GenSockaddr::V6(ref mut v6a) => {
            let v6len = size_of::<interface::SockaddrV6>() as u32;

            let fullcopylen = interface::rust_min(initaddrlen, v6len);
            unsafe {
                std::ptr::copy(
                    (v6a) as *mut interface::SockaddrV6 as *mut u8,
                    copyoutaddr,
                    initaddrlen as usize,
                )
            };
            unsafe {
                *addrlen = interface::rust_max(v6len, fullcopylen);
            }
        }
    }
}

pub fn get_pollstruct_slice<'a>(
    generic_argument: u64,
    nfds: usize,
) -> Result<&'a mut [PollStruct], i32> {
    let pollstructptr = generic_argument as *mut PollStruct;
    if !pollstructptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(pollstructptr, nfds) });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_epollevent_slice<'a>(
    generic_argument: u64,
    nfds: i32,
) -> Result<&'a mut [EpollEvent], i32> {
    let epolleventptr = generic_argument as *mut EpollEvent;
    if !epolleventptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(epolleventptr, nfds as usize) });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_slice_from_string<'a>(generic_argument: u64, len: usize) -> Result<&'a mut [u8], i32> {
    let bufptr = generic_argument as *mut u8;
    if bufptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(bufptr, len as usize) });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_epollevent<'a>(generic_argument: u64) -> Result<&'a mut EpollEvent, i32> {
    let epolleventptr = generic_argument as *mut EpollEvent;
    if !epolleventptr.is_null() {
        return Ok(unsafe { &mut *epolleventptr });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_socklen_t_ptr(generic_argument: u64) -> Result<u32, i32> {
    let socklenptr = generic_argument as *mut u32;
    if !socklenptr.is_null() {
        return Ok(unsafe { *socklenptr });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

//arg checked for nullity beforehand
pub fn get_int_from_intptr(generic_argument: u64) -> i32 {
    return unsafe { *(generic_argument as *mut i32) };
}

pub fn copy_out_intptr(generic_argument: u64, intval: i32) {
    unsafe {
        *(generic_argument as *mut i32) = intval;
    }
}

pub fn duration_fromtimeval(generic_argument: u64) -> Result<Option<interface::RustDuration>, i32> {
    let pointer = generic_argument as *mut timeval;
    if !pointer.is_null() {
        let times = unsafe { &mut *pointer };
        return Ok(Some(interface::RustDuration::new(
            times.tv_sec as u64,
            times.tv_usec as u32 * 1000,
        )));
    } else {
        return Ok(None);
    }
}

pub fn get_timerval<'a>(generic_argument: u64) -> Result<&'a mut timeval, i32> {
    let pointer = generic_argument as *mut timeval;
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_itimerval<'a>(generic_argument: u64) -> Result<Option<&'a mut ITimerVal>, i32> {
    let pointer = generic_argument as *mut ITimerVal;
    if !pointer.is_null() {
        Ok(Some(unsafe { &mut *pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_constitimerval<'a>(generic_argument: u64) -> Result<Option<&'a ITimerVal>, i32> {
    let pointer = generic_argument as *const ITimerVal;
    if !pointer.is_null() {
        Ok(Some(unsafe { &*pointer }))
    } else {
        Ok(None)
    }
}

pub fn duration_fromtimespec(generic_argument: u64) -> Result<interface::RustDuration, i32> {
    let pointer = generic_argument as *mut TimeSpec;
    if !pointer.is_null() {
        let times = unsafe { &mut *pointer };
        if times.tv_nsec < 0 || times.tv_nsec >= 1000000000 {
            return Err(syscall_error(
                Errno::EINVAL,
                "timedwait",
                "nanosecond count was negative or more than 1 billion",
            ));
        }
        return Ok(interface::RustDuration::new(
            times.tv_sec as u64,
            times.tv_nsec as u32 * 1000000000,
        ));
    } else {
        return Err(syscall_error(
            Errno::EFAULT,
            "timedwait",
            "input timespec is null",
        ));
    }
}

pub fn get_timespec<'a>(generic_argument: u64) -> Result<&'a timespec, i32> {
    let pointer = generic_argument as *mut timespec;
    if !pointer.is_null() {
        return Ok(unsafe { &*pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_duration_from_millis(
    generic_argument: u64,
) -> Result<Option<interface::RustDuration>, i32> {
    let posstimemillis = get_int(generic_argument);
    match posstimemillis {
        Ok(timemillis) => {
            if timemillis >= 0 {
                Ok(Some(interface::RustDuration::from_millis(
                    timemillis as u64,
                )))
            } else {
                Ok(None)
            }
        }
        Err(e) => Err(e),
    }
}

pub fn arg_nullity(generic_argument: u64) -> bool {
    (generic_argument as *const u8).is_null()
}

pub fn get_sigactionstruct<'a>(
    generic_argument: u64,
) -> Result<Option<&'a mut SigactionStruct>, i32> {
    let pointer = generic_argument as *mut SigactionStruct;

    if !pointer.is_null() {
        Ok(Some(unsafe { &mut *pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_constsigactionstruct<'a>(
    generic_argument: u64,
) -> Result<Option<&'a SigactionStruct>, i32> {
    let pointer = generic_argument as *const SigactionStruct;

    if !pointer.is_null() {
        Ok(Some(unsafe { &*pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_sigsett<'a>(generic_argument: u64) -> Result<Option<&'a mut SigsetType>, i32> {
    let pointer = generic_argument as *mut u64;

    if !pointer.is_null() {
        Ok(Some(unsafe { &mut *pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_constsigsett<'a>(generic_argument: u64) -> Result<Option<&'a SigsetType>, i32> {
    let pointer = generic_argument as *const SigsetType;

    if !pointer.is_null() {
        Ok(Some(unsafe { &*pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_iovecstruct(generic_argument: u64) -> Result<*const interface::IovecStruct, i32> {
    let data = generic_argument as *const interface::IovecStruct;
    if !data.is_null() {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}
