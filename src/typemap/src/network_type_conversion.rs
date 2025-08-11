use cage::{get_cage, translate_vmmap_addr};
pub use libc::*;
pub use std::time::Duration;
use std::ptr;
use sysdefs::*;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::data::fs_struct::PipeArray;
use sysdefs::constants::fs_const::LIND_ROOT;


/// Checks whether a user-space argument is null.
pub fn sc_convert_arg_nullity(arg: u64, arg_cageid: u64, cageid: u64) -> bool {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }
    
    (arg as *const u8).is_null()
}

/// Converts a user-space pointer into a mutable slice of `PollStruct`.
pub fn sc_convert_pollstruct_slice<'a>(
    arg: u64,
    arg_cageid: u64, 
    cageid: u64,
    nfds: usize
) -> Result<&'a mut [PollStruct], i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let pollstructptr = arg as *mut PollStruct;
    if !pollstructptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(pollstructptr, nfds) });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

/// Converts a user-space pointer into a mutable reference to `EpollEvent`.
pub fn sc_convert_epollevent<'a>(arg: u64, arg_cageid: u64, cageid: u64) -> Result<&'a mut EpollEvent, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let cage = get_cage(arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    let epolleventptr = addr as *mut EpollEvent;
    if !epolleventptr.is_null() {
        return Ok(unsafe { &mut *epolleventptr });
    }
    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

/// Converts a user-space pointer into a mutable slice of `EpollEvent`.
pub fn sc_convert_epollevent_slice<'a>(
    arg: u64,
    arg_cageid: u64, 
    cageid: u64,
    nfds: i32,
) -> Result<&'a mut [EpollEvent], i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }
    let cage = get_cage(arg_cageid).unwrap();
    let addr = translate_vmmap_addr(&cage, arg).unwrap();
    let epolleventptr = addr as *mut EpollEvent;

    if !epolleventptr.is_null() {
        return Ok(unsafe { std::slice::from_raw_parts_mut(epolleventptr, nfds as usize) });
    }

    return Err(syscall_error(
        Errno::EFAULT,
        "dispatcher",
        "input data not valid",
    ));
}

/// Converts a user-space pointer into a mutable reference to `SockPair`.
pub fn sc_convert_sockpair<'a>(arg: u64, arg_cageid: u64, cageid: u64,) -> Result<&'a mut SockPair, i32> {
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

pub fn fill(bufptr: *mut u8, count: usize, values: &Vec<u8>) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts_mut(bufptr, count) };
    slice.copy_from_slice(&values[..count]);
    count as i32
}

/// Converts a user-space pointer into an optional mutable reference to `fd_set`.
pub fn sc_convert_fdset(arg: u64, arg_cageid: u64, cageid: u64) -> Result<Option<&'static mut fd_set>, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let data = arg as *mut libc::fd_set;
    if !data.is_null() {
        let internal_fds = unsafe { &mut *(data as *mut fd_set) };
        return Ok(Some(internal_fds));
    }
    return Ok(None);
}

/// Converts a user-space `timeval` pointer into an optional `Duration`.
pub fn sc_convert_duration_fromtimeval(arg: u64, arg_cageid: u64, cageid: u64) -> Result<Option<Duration>, i32> {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let pointer = arg as *mut timeval;
    if !pointer.is_null() {
        let times = unsafe { &mut *pointer };
        return Ok(Some(Duration::new(
            times.tv_sec as u64,
            times.tv_usec as u32 * 1000,
        )));
    } else {
        return Ok(None);
    }
}

/// Converts a user-space socket address into a host-compatible `sockaddr` used for syscalls.
pub fn sc_convert_host_sockaddr(arg: *mut u8, arg_cageid: u64, cageid: u64) -> (*mut sockaddr, u32) {
    #[cfg(feature = "secure")]
    {
        if !validate_cageid(arg_cageid, cageid) {
            return -1;
        }
    }

    let mut saddr = SockAddr::clone_to_sockaddr(arg);

    if (saddr.sun_family as i32) == AF_UNIX {
        unsafe {
            let sun_path_ptr = saddr.sun_path.as_mut_ptr();
            let path_len = strlen(sun_path_ptr);
            let lind_root_len = LIND_ROOT.len();
            let new_path_len = path_len + lind_root_len;

            if new_path_len < 108 {
                memmove(
                    sun_path_ptr.add(lind_root_len) as *mut c_void,
                    sun_path_ptr as *const c_void,
                    path_len,
                );
                memcpy(
                    sun_path_ptr as *mut c_void,
                    LIND_ROOT.as_ptr() as *const c_void,
                    lind_root_len,
                );
                memset(
                    sun_path_ptr.add(new_path_len) as *mut c_void,
                    0,
                    108 - new_path_len,
                );
            }
        }
    }
    let boxed = Box::new(saddr);
    let ptr = Box::into_raw(boxed) as *mut sockaddr_un;
    let ptr = ptr.cast::<sockaddr>();
    let len = unsafe { (*(ptr as *mut SockAddr)).get_len() };
    (ptr, len)
}

/// Copies a socket address structure from the kernel into user space based on the given address family.
pub fn sc_convert_copy_out_sockaddr(
    addr_arg: u64,    
    addr_arg1: u64,   
    family: u16,
) {
    let copyoutaddr = addr_arg as *mut u8;
    let addrlen = addr_arg1 as *mut u32;

    assert!(!copyoutaddr.is_null());
    assert!(!addrlen.is_null());

    let initaddrlen = unsafe { *addrlen };

    let (src_ptr, actual_len): (*const u8, u32) = match family as i32 {
        AF_INET => {
            let v4 = SockAddr::new_ipv4();
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