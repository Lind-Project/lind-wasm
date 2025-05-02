#![allow(dead_code)]
use crate::interface;

use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::data::fs_struct::*;
use sysdefs::data::net_struct::*;
use sysdefs::data::{fs_struct, net_struct};

use libc::*;
use std::ffi::CStr;
use std::io;
use std::io::{Read, Write};
use std::ptr::null;
use std::str::Utf8Error;

const SIZEOF_SOCKADDR: u32 = 16;

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

/// Returns a mutable reference to an `i32` value at the given memory address
///
/// ## Arguments
/// generic_argument: A `u64` memory address that has already been translated to host
///
/// ## Return
/// A mutable reference to the resolved `i32` value, or `0` when address is `NULL`
pub fn get_i32_ref<'a>(generic_argument: u64) -> Result<&'a mut i32, i32> {
    // Handle NULL explicitly to avoid panic when resolving status pointer from invalid address
    let pointer = if generic_argument == 0 {
        &mut 0
    } else {
        unsafe { &mut *((generic_argument) as *mut i32) }
    };

    return Ok(pointer);
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

pub fn get_sockaddr(generic_argument: u64, addrlen: u32) -> Result<net_struct::GenSockaddr, i32> {
    let pointer = generic_argument as *const SockaddrDummy;
    if !pointer.is_null() {
        let tmpsock = unsafe { &*pointer };
        match tmpsock.sa_family {
            /*AF_UNIX*/
            1 => {
                if addrlen < SIZEOF_SOCKADDR
                    || addrlen > size_of::<net_struct::SockaddrUnix>() as u32
                {
                    return Err(syscall_error(
                        Errno::EINVAL,
                        "dispatcher",
                        "input length incorrect for family of sockaddr",
                    ));
                }
                let unix_ptr = pointer as *const net_struct::SockaddrUnix;
                return Ok(net_struct::GenSockaddr::Unix(unsafe { *unix_ptr }));
            }
            /*AF_INET*/
            2 => {
                if addrlen < size_of::<net_struct::SockaddrV4>() as u32 {
                    return Err(syscall_error(
                        Errno::EINVAL,
                        "dispatcher",
                        "input length too small for family of sockaddr",
                    ));
                }
                let v4_ptr = pointer as *const net_struct::SockaddrV4;
                return Ok(net_struct::GenSockaddr::V4(unsafe { *v4_ptr }));
            }
            /*AF_INET6*/
            30 => {
                if addrlen < size_of::<net_struct::SockaddrV6>() as u32 {
                    return Err(syscall_error(
                        Errno::EINVAL,
                        "dispatcher",
                        "input length too small for family of sockaddr",
                    ));
                }
                let v6_ptr = pointer as *const net_struct::SockaddrV6;
                return Ok(net_struct::GenSockaddr::V6(unsafe { *v6_ptr }));
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
) -> Result<net_struct::GenSockaddr, i32> {
    let received = generic_argument as *mut SockaddrDummy;
    let received_addrlen = (generic_argument1 as *mut u32) as u32;
    let tmpsock = unsafe { &*received };
    match tmpsock.sa_family {
        /*AF_UNIX*/
        1 => {
            if received_addrlen < SIZEOF_SOCKADDR
                || received_addrlen > size_of::<net_struct::SockaddrUnix>() as u32
            {
                return Err(syscall_error(
                    Errno::EINVAL,
                    "dispatcher",
                    "input length incorrect for family of sockaddr",
                ));
            }
            let unix_addr = net_struct::GenSockaddr::Unix(net_struct::SockaddrUnix::default());
            return Ok(unix_addr);
        }
        /*AF_INET*/
        2 => {
            if received_addrlen < size_of::<net_struct::SockaddrV4>() as u32 {
                return Err(syscall_error(
                    Errno::EINVAL,
                    "dispatcher",
                    "input length too small for family of sockaddr",
                ));
            }
            let v4_addr = net_struct::GenSockaddr::V4(net_struct::SockaddrV4::default());
            return Ok(v4_addr);
        }
        /*AF_INET6*/
        30 => {
            if received_addrlen < size_of::<net_struct::SockaddrV6>() as u32 {
                return Err(syscall_error(
                    Errno::EINVAL,
                    "dispatcher",
                    "input length too small for family of sockaddr",
                ));
            }
            let v6_addr = net_struct::GenSockaddr::V6(net_struct::SockaddrV6::default());
            return Ok(v6_addr);
        }
        _ => {
            let null_addr = net_struct::GenSockaddr::Unix(net_struct::SockaddrUnix::default());
            return Ok(null_addr);
        }
    }
}

pub fn copy_out_sockaddr(
    generic_argument: u64,
    generic_argument1: u64,
    gensock: net_struct::GenSockaddr,
) {
    let copyoutaddr = (generic_argument as *mut SockaddrDummy) as *mut u8;
    let addrlen = generic_argument1 as *mut u32;
    assert!(!copyoutaddr.is_null());
    assert!(!addrlen.is_null());
    let initaddrlen = unsafe { *addrlen };
    let mut mutgensock = gensock;
    match mutgensock {
        net_struct::GenSockaddr::Unix(ref mut unixa) => {
            let unixlen = size_of::<net_struct::SockaddrUnix>() as u32;

            let fullcopylen = interface::rust_min(initaddrlen, unixlen);
            unsafe {
                std::ptr::copy(
                    (unixa) as *mut net_struct::SockaddrUnix as *mut u8,
                    copyoutaddr,
                    initaddrlen as usize,
                )
            };
            unsafe {
                *addrlen = interface::rust_max(unixlen, fullcopylen);
            }
        }

        net_struct::GenSockaddr::V4(ref mut v4a) => {
            let v4len = size_of::<net_struct::SockaddrV4>() as u32;

            let fullcopylen = interface::rust_min(initaddrlen, v4len);

            unsafe {
                std::ptr::copy(
                    (v4a) as *mut net_struct::SockaddrV4 as *mut u8,
                    copyoutaddr,
                    initaddrlen as usize,
                )
            };
            unsafe {
                *addrlen = interface::rust_max(v4len, fullcopylen);
            }
        }

        net_struct::GenSockaddr::V6(ref mut v6a) => {
            let v6len = size_of::<net_struct::SockaddrV6>() as u32;

            let fullcopylen = interface::rust_min(initaddrlen, v6len);
            unsafe {
                std::ptr::copy(
                    (v6a) as *mut net_struct::SockaddrV6 as *mut u8,
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

pub fn get_iovecstruct(generic_argument: u64) -> Result<*const fs_struct::IovecStruct, i32> {
    let data = generic_argument as *const fs_struct::IovecStruct;
    if !data.is_null() {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}
