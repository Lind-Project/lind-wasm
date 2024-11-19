#![allow(dead_code)]
use crate::interface;
use crate::interface::errnos::{syscall_error, Errno};

const SIZEOF_SOCKADDR: u32 = 16;

//redefining the FSData struct in this file so that we maintain flow of program
//derive eq attributes for testing whether the structs equal other fsdata structs from stat/fstat
#[derive(Eq, PartialEq, Default)]
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
#[derive(Eq, PartialEq, Default)]
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

#[derive(Eq, PartialEq, Default, Copy, Clone)]
#[repr(C)]
pub struct PipeArray {
    pub readfd: i32,
    pub writefd: i32,
}

#[derive(Eq, PartialEq, Default, Copy, Clone)]
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

#[repr(C)]
pub struct TimeVal {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

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

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct SigactionStruct {
    pub sa_handler: u32,
    pub sa_mask: SigsetType,
    pub sa_flags: i32,
}

pub type IovecStruct = libc::iovec;

//redefining the Arg union to maintain the flow of the program
#[derive(Copy, Clone)]
#[repr(C)]
pub union Arg {
    pub dispatch_int: i32,
    pub dispatch_uint: u32,
    pub dispatch_ulong: u64,
    pub dispatch_long: i64,
    pub dispatch_usize: usize, //For types not specified to be a given length, but often set to word size (i.e. size_t)
    pub dispatch_isize: isize, //For types not specified to be a given length, but often set to word size (i.e. off_t)
    pub dispatch_cbuf: *const u8, //Typically corresponds to an immutable void* pointer as in write
    pub dispatch_mutcbuf: *mut u8, //Typically corresponds to a mutable void* pointer as in read
    pub dispatch_cstr: *const i8, //Typically corresponds to a passed in string of type char*, as in open
    pub dispatch_cstrarr: *const *const i8, //Typically corresponds to a passed in string array of type char* const[] as in execve
    pub dispatch_rlimitstruct: *mut Rlimit,
    pub dispatch_statdatastruct: *mut StatData,
    pub dispatch_fsdatastruct: *mut FSData,
    pub dispatch_shmidstruct: *mut ShmidsStruct,
    pub dispatch_constsockaddrstruct: *const SockaddrDummy,
    pub dispatch_sockaddrstruct: *mut SockaddrDummy,
    pub dispatch_socklen_t_ptr: *mut u32,
    pub dispatch_intptr: *mut i32,
    pub dispatch_pollstructarray: *mut PollStruct,
    pub dispatch_epollevent: *mut EpollEvent,
    pub dispatch_structtimeval: *mut TimeVal,
    pub dispatch_structtimespec: *mut TimeSpec,
    pub dispatch_pipearray: *mut PipeArray,
    pub dispatch_sockpair: *mut SockPair,
    pub dispatch_ioctlptrunion: IoctlPtrUnion,
    pub dispatch_sigactionstruct: *mut SigactionStruct,
    pub dispatch_constsigactionstruct: *const SigactionStruct,
    pub dispatch_sigsett: *mut SigsetType,
    pub dispatch_constsigsett: *const SigsetType,
    pub dispatch_structitimerval: *mut ITimerVal,
    pub dispatch_conststructitimerval: *const ITimerVal,
    pub dispatch_fdset: *mut libc::fd_set,
    pub dispatch_constiovecstruct: *const interface::IovecStruct,
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

pub fn get_int(union_argument: Arg) -> Result<i32, i32> {
    let data = unsafe { union_argument.dispatch_int };
    let mut type_checker = Arg { dispatch_long: 0 };
    //turn part of the union into 0xffffffff, but, Rust
    //does not like just using the hex value so we are forced to use
    //a value of -1
    type_checker.dispatch_int = -1;
    if (unsafe { union_argument.dispatch_long } & !unsafe { type_checker.dispatch_long }) == 0 {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EINVAL,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_uint(union_argument: Arg) -> Result<u32, i32> {
    let data = unsafe { union_argument.dispatch_uint };
    let mut type_checker = Arg { dispatch_ulong: 0 };
    type_checker.dispatch_uint = 0xffffffff;
    if (unsafe { union_argument.dispatch_ulong } & !unsafe { type_checker.dispatch_ulong }) == 0 {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EINVAL,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_long(union_argument: Arg) -> Result<i64, i32> {
    return Ok(unsafe { union_argument.dispatch_long }); //this should not return error
}

pub fn get_ulong(union_argument: Arg) -> Result<u64, i32> {
    return Ok(unsafe { union_argument.dispatch_ulong }); //this should not return error
}

pub fn get_isize(union_argument: Arg) -> Result<isize, i32> {
    // also should not return error
    return Ok(unsafe { union_argument.dispatch_isize });
}

pub fn get_usize(union_argument: Arg) -> Result<usize, i32> {
    //should not return an error
    return Ok(unsafe { union_argument.dispatch_usize });
}

pub fn get_cbuf(union_argument: Arg) -> Result<*const u8, i32> {
    let data = unsafe { union_argument.dispatch_cbuf };
    if !data.is_null() {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_mutcbuf(union_argument: Arg) -> Result<*mut u8, i32> {
    let data = unsafe { union_argument.dispatch_mutcbuf };
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
pub fn get_mutcbuf_null(union_argument: Arg) -> Result<Option<*mut u8>, i32> {
    let data = unsafe { union_argument.dispatch_mutcbuf };
    if !data.is_null() {
        return Ok(Some(data));
    }
    return Ok(None);
}

pub fn get_fdset(union_argument: Arg) -> Result<Option<&'static mut interface::FdSet>, i32> {
    let data: *mut libc::fd_set = unsafe { union_argument.dispatch_fdset };
    if !data.is_null() {
        let internal_fds: &mut interface::FdSet = interface::FdSet::new_from_ptr(data);
        return Ok(Some(internal_fds));
    }
    return Ok(None);
}

pub fn get_cstr<'a>(union_argument: Arg) -> Result<&'a str, i32> {
    //first we check that the pointer is not null
    //and then we check so that we can get data from the memory

    let pointer = unsafe { union_argument.dispatch_cstr };
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

pub fn get_cstrarr<'a>(union_argument: Arg) -> Result<Vec<&'a str>, i32> {
    //iterate though the pointers in a function and:
    //  1: check that the pointer is not null
    //  2: push the data from that pointer onto the vector being returned
    //once we encounter a null pointer, we know that we have either hit the end of the array or another null pointer in the memory

    let mut pointer = unsafe { union_argument.dispatch_cstrarr };
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

pub fn get_iovecstruct(union_argument: Arg) -> Result<*const interface::IovecStruct, i32> {
    let data = unsafe { union_argument.dispatch_constiovecstruct };
    if !data.is_null() {
        return Ok(data);
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_statdatastruct<'a>(union_argument: Arg) -> Result<&'a mut StatData, i32> {
    let pointer = unsafe { union_argument.dispatch_statdatastruct };
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_fsdatastruct<'a>(union_argument: Arg) -> Result<&'a mut FSData, i32> {
    let pointer = unsafe { union_argument.dispatch_fsdatastruct };
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_shmidstruct<'a>(union_argument: Arg) -> Result<&'a mut ShmidsStruct, i32> {
    let pointer = unsafe { union_argument.dispatch_shmidstruct };
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_ioctlptrunion(union_argument: Arg) -> Result<IoctlPtrUnion, i32> {
    return Ok(unsafe { union_argument.dispatch_ioctlptrunion });
}

pub fn get_ioctl_int<'a>(ptrunion: IoctlPtrUnion) -> Result<i32, i32> {
    let pointer = unsafe { ptrunion.int_ptr };
    if !pointer.is_null() {
        return Ok(unsafe { *pointer });
    }
    return Err(syscall_error(Errno::EFAULT, "ioctl", "argp is not valid"));
}

pub fn get_ioctl_char<'a>(ptrunion: IoctlPtrUnion) -> Result<u8, i32> {
    let pointer = unsafe { ptrunion.c_char_ptr };
    if !pointer.is_null() {
        return Ok(unsafe { *pointer });
    }
    return Err(syscall_error(Errno::EFAULT, "ioctl", "argp is not valid"));
}

/// Given the vector of tuples produced from getdents_syscall, each of which consists of
/// a ClippedDirent struct and a u8 vector representing the name, and also given the
/// pointer to the base of the buffer to which the getdents structs should be copied,
/// populate said buffer with these getdents structs and the names at the requisite locations
///
/// We assume a number of things about the tuples that are input:
///
/// 1. The name in the u8 vec is null terminated
/// 2. After being null terminated it is then padded to the next highest 8 byte boundary
/// 3. After being padded, the last byte of padding is populated with DT_UNKNOWN (0) for now,
/// as the d_type field does not have to be fully implemented for getdents to be POSIX compliant
/// 4. All fields in the clipped dirent,  are correctly filled--i.e. d_off has the correct offset
/// of the next struct in the buffer and d_reclen has the length of the struct with the padded name
/// 5. The number of tuples in the vector is such that they all fit in the buffer
///
/// There is enough information to produce a tuple vector that can satisfy these assumptions well
/// in getdents syscall, and thus all the work to satisfy these assumptions should be done there
pub fn pack_dirents(dirtuplevec: Vec<(ClippedDirent, Vec<u8>)>, baseptr: *mut u8) {
    let mut curptr = baseptr;

    //for each tuple we write in the ClippedDirent struct, and then the padded name vec
    for dirtuple in dirtuplevec {
        //get pointer to start of next dirent in the buffer as a ClippedDirent pointer
        let curclippedptr = curptr as *mut ClippedDirent;
        //turn that pointer into a rust reference
        let curwrappedptr = unsafe { &mut *curclippedptr };
        //assign to the data that reference points to with the value of the ClippedDirent from the tuple
        *curwrappedptr = dirtuple.0;

        //advance pointer by the size of one ClippedDirent, std::mem::size_of should be added into the interface
        curptr = curptr.wrapping_offset(size_of::<ClippedDirent>() as isize);

        //write, starting from this advanced location, the u8 vec representation of the name
        unsafe { curptr.copy_from(dirtuple.1.as_slice().as_ptr(), dirtuple.1.len()) };

        //advance pointer by the size of name, which we assume to be null terminated and padded correctly
        //and thus we are finished with this struct
        curptr = curptr.wrapping_offset(dirtuple.1.len() as isize);
    }
}

pub fn get_pipearray<'a>(union_argument: Arg) -> Result<&'a mut PipeArray, i32> {
    let pointer = unsafe { union_argument.dispatch_pipearray };
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_sockpair<'a>(union_argument: Arg) -> Result<&'a mut SockPair, i32> {
    let pointer = unsafe { union_argument.dispatch_sockpair };
    if !pointer.is_null() {
        return Ok(unsafe { &mut *pointer });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_sockaddr(union_argument: Arg, addrlen: u32) -> Result<interface::GenSockaddr, i32> {
    let pointer = unsafe { union_argument.dispatch_constsockaddrstruct };
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
            _ => {
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

pub fn copy_out_sockaddr(union_argument: Arg, len_argument: Arg, gensock: interface::GenSockaddr) {
    let copyoutaddr = unsafe { union_argument.dispatch_sockaddrstruct } as *mut u8;
    let addrlen = unsafe { len_argument.dispatch_socklen_t_ptr };
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
                    fullcopylen as usize,
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
                    fullcopylen as usize,
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
                    fullcopylen as usize,
                )
            };
            unsafe {
                *addrlen = interface::rust_max(v6len, fullcopylen);
            }
        }
    }
}

pub fn get_pollstruct_slice<'a>(
    union_argument: Arg,
    nfds: usize,
) -> Result<&'a mut [PollStruct], i32> {
    let pollstructptr = unsafe { union_argument.dispatch_pollstructarray };
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
    union_argument: Arg,
    nfds: i32,
) -> Result<&'a mut [EpollEvent], i32> {
    let epolleventptr = unsafe { union_argument.dispatch_epollevent };
    if !epolleventptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(epolleventptr, nfds as usize) });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_slice_from_string<'a>(union_argument: Arg, len: usize) -> Result<&'a mut [u8], i32> {
    let bufptr = unsafe { union_argument.dispatch_mutcbuf };
    if bufptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(bufptr, len as usize) });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_epollevent<'a>(union_argument: Arg) -> Result<&'a EpollEvent, i32> {
    let epolleventptr = unsafe { union_argument.dispatch_epollevent };
    if !epolleventptr.is_null() {
        return Ok(unsafe { &*epolleventptr });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

pub fn get_socklen_t_ptr(union_argument: Arg) -> Result<u32, i32> {
    let socklenptr = unsafe { union_argument.dispatch_socklen_t_ptr };
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
pub fn get_int_from_intptr(union_argument: Arg) -> i32 {
    return unsafe { *union_argument.dispatch_intptr };
}

pub fn copy_out_intptr(union_argument: Arg, intval: i32) {
    unsafe {
        *union_argument.dispatch_intptr = intval;
    }
}

pub fn duration_fromtimeval(union_argument: Arg) -> Result<Option<interface::RustDuration>, i32> {
    let pointer = unsafe { union_argument.dispatch_structtimeval };
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

pub fn get_itimerval<'a>(union_argument: Arg) -> Result<Option<&'a mut ITimerVal>, i32> {
    let pointer = unsafe { union_argument.dispatch_structitimerval };
    if !pointer.is_null() {
        Ok(Some(unsafe { &mut *pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_constitimerval<'a>(union_argument: Arg) -> Result<Option<&'a ITimerVal>, i32> {
    let pointer = unsafe { union_argument.dispatch_conststructitimerval };
    if !pointer.is_null() {
        Ok(Some(unsafe { &*pointer }))
    } else {
        Ok(None)
    }
}

pub fn duration_fromtimespec(union_argument: Arg) -> Result<interface::RustDuration, i32> {
    let pointer = unsafe { union_argument.dispatch_structtimespec };
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

pub fn get_duration_from_millis(
    union_argument: Arg,
) -> Result<Option<interface::RustDuration>, i32> {
    let posstimemillis = get_int(union_argument);
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

pub fn arg_nullity(union_argument: &Arg) -> bool {
    unsafe { union_argument.dispatch_cbuf }.is_null()
}

pub fn get_sigactionstruct<'a>(
    union_argument: Arg,
) -> Result<Option<&'a mut SigactionStruct>, i32> {
    let pointer = unsafe { union_argument.dispatch_sigactionstruct };

    if !pointer.is_null() {
        Ok(Some(unsafe { &mut *pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_constsigactionstruct<'a>(
    union_argument: Arg,
) -> Result<Option<&'a SigactionStruct>, i32> {
    let pointer = unsafe { union_argument.dispatch_constsigactionstruct };

    if !pointer.is_null() {
        Ok(Some(unsafe { &*pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_sigsett<'a>(union_argument: Arg) -> Result<Option<&'a mut SigsetType>, i32> {
    let pointer = unsafe { union_argument.dispatch_sigsett };

    if !pointer.is_null() {
        Ok(Some(unsafe { &mut *pointer }))
    } else {
        Ok(None)
    }
}

pub fn get_constsigsett<'a>(union_argument: Arg) -> Result<Option<&'a SigsetType>, i32> {
    let pointer = unsafe { union_argument.dispatch_constsigsett };

    if !pointer.is_null() {
        Ok(Some(unsafe { &*pointer }))
    } else {
        Ok(None)
    }
}
