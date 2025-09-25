use sysdefs::data::net_struct::{SockAddr, SockPair};
use libc::{sockaddr, strlen, sockaddr_un, sockaddr_in, sockaddr_in6};
use sysdefs::constants::{Errno, syscall_error};

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
