use cage::get_cage;

use sysdefs::constants::net_const;
use sysdefs::constants::sys_const;
use typemap::path_conv::LIND_ROOT;
use typemap::syscall_conv::*;
use fdtables;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};

use std::collections::HashSet;
use std::collections::HashMap;
use std::convert::TryInto;
use dashmap::mapref::entry;
use parking_lot::Mutex;
use lazy_static::lazy_static;
use std::io::{Read, Write};
use std::io;
use std::mem::size_of;
use libc::*;
use std::ffi::CString;
use std::ffi::CStr;
use std::sync::Arc;
use cage::memory::mem_helper::*;

// use crate::safeposix::filesystem::normpath;

use libc::*;
use std::{os::fd::RawFd, ptr};
// use bit_set::BitSet;

const FDKIND_KERNEL: u32 = 0;
const FDKIND_IMPIPE: u32 = 1;
const FDKIND_IMSOCK: u32 = 2;

lazy_static! {
    // A hashmap used to store epoll mapping relationships 
    // <virtual_epfd <kernel_fd, virtual_fd>> 
    static ref REAL_EPOLL_MAP: Mutex<HashMap<u64, HashMap<i32, u64>>> = Mutex::new(HashMap::new());
}


/// Reference to Linux: https://man7.org/linux/man-pages/man2/socket.2.html
///
/// The Linux `socket()` syscall creates an endpoint for communication and returns a file descriptor
/// for the newly created socket. This implementation wraps the syscall and registers the resulting
/// file descriptor in our virtual file descriptor table (`fdtables`) under the current cage.
///
/// The `fdtables` system manages per-cage file descriptors and tracks their lifecycle. No special flags
/// such as `O_CLOEXEC` are applied at this stage.
///
/// Input:
///     - cageid: current cageid
///     - domain_arg: Communication domain (e.g., AF_INET, AF_UNIX)
///     - socktype_arg: Socket type (e.g., SOCK_STREAM, SOCK_DGRAM)
///     - protocol_arg: Protocol to be used (usually 0)
///
/// Return:
///     - On success: a newly allocated virtual file descriptor within the current cage
///     - On failure: a negative errno value indicating the syscall error
pub fn socket_syscall(
    cageid: u64,
    domain_arg: u64,
    domain_cageid: u64,
    socktype_arg: u64,
    socktype_cageid: u64,
    protocol_arg: u64,
    protocol_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {

    let domain = sc_convert_sysarg_to_i32(domain_arg, domain_cageid, cageid);
    let socktype = sc_convert_sysarg_to_i32(socktype_arg, socktype_cageid, cageid);
    let protocol = sc_convert_sysarg_to_i32(protocol_arg, protocol_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "socket_syscall", "Invalide Cage ID");
    }

    let kernel_fd = unsafe { libc::socket(domain, socktype, protocol) };
       
        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "socket");
        }

        return fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, false, 0).unwrap() as i32;
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/connect.2.html
///
/// The Linux `connect()` syscall connects a socket referred to by a file descriptor to the specified
/// address. This implementation resolves the provided virtual file descriptor and memory address from
/// the calling cage and performs the corresponding kernel operation. If the socket is a UNIX domain
/// socket (AF_UNIX), the path is modified to include the sandbox root path (`LIND_ROOT`) to ensure the
/// socket file resides within the correct namespace.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor for the socket to be connected
///     - addr_arg: Pointer to a `sockaddr_un` structure containing the target address
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn connect_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {

    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);
    
    if !(sc_unusedarg(arg3, arg3_cageid)
        &&sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "connect_syscall", "Invalide Cage ID");
    }
    
    // Handle sockaddr conversion; if NULL, use empty pointer
    let (finalsockaddr, addrlen) = if addr.is_null() {
        (std::ptr::null::<libc::sockaddr_un>() as *mut libc::sockaddr_un, 0)
    } else {
        let mut sockaddr_un: sockaddr_un = sockaddr_un {
            sun_family: 0 as u16,
            sun_path: [0; 108],
        };

        let sockaddr_un_ptr: *mut sockaddr_un = &mut sockaddr_un;

        unsafe {
            // Copy user's sockaddr to local buffer
            std::ptr::copy_nonoverlapping(addr as *mut libc::sockaddr_un, sockaddr_un_ptr, 1);

            // If AF_UNIX socket, rewrite sun_path with LIND_ROOT prefix
            if (*sockaddr_un_ptr).sun_family as i32 == AF_UNIX {
                let sun_path_ptr = (*sockaddr_un_ptr).sun_path.as_mut_ptr();
                let path_len = strlen(sun_path_ptr);
                const LIND_ROOT_LEN: usize = LIND_ROOT.len();
        
                let new_path_len = path_len + LIND_ROOT_LEN;
        
                if new_path_len < 108 {
                    memmove(
                        sun_path_ptr.add(LIND_ROOT_LEN) as *mut libc::c_void,
                        sun_path_ptr as *const libc::c_void,
                        path_len,
                    );
        
                    libc::memcpy(
                        sun_path_ptr as *mut libc::c_void,
                        LIND_ROOT.as_ptr() as *const libc::c_void,
                        LIND_ROOT_LEN,
                    );

                    libc::memset(
                        sun_path_ptr.add(new_path_len) as *mut libc::c_void,
                        0,
                        108 - new_path_len,
                    );
                }
            }
        }
        (sockaddr_un_ptr, size_of::<libc::sockaddr_un>() as u32)            
    };
    
    let finalsockaddr = finalsockaddr as *mut libc::sockaddr;

    let ret = unsafe { libc::connect(fd as i32, finalsockaddr, addrlen as u32) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "connect");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/bind.2.html
///
/// The Linux `bind()` syscall assigns a local address to a socket, which is required before a socket
/// can accept incoming connections. This implementation first converts the virtual file descriptor and
/// socket address from the calling cage into kernel-visible forms. If the address is a UNIX domain
/// socket (AF_UNIX), the path is rewritten to include a sandbox root (`LIND_ROOT`) to enforce proper
/// isolation within the namespace.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor to be bound
///     - addr_arg: Pointer to a `sockaddr_un` structure containing the local address
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn bind_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "bind_syscall", "Invalide Cage ID");
    }

    // Handle sockaddr conversion; if NULL, use empty pointerv
    let (finalsockaddr, addrlen) = if addr.is_null() {
        (std::ptr::null::<libc::sockaddr_un>() as *mut libc::sockaddr_un, 0)
    } else {
        let mut sockaddr_un: sockaddr_un = sockaddr_un {
            sun_family: 0 as u16,
            sun_path: [0; 108],
        };

        let sockaddr_un_ptr: *mut sockaddr_un = &mut sockaddr_un;

        unsafe {
            // Copy user's sockaddr to local buffer
            std::ptr::copy_nonoverlapping(addr as *mut libc::sockaddr_un, sockaddr_un_ptr, 1);

            // If AF_UNIX socket, rewrite sun_path with LIND_ROOT prefix
            if (*sockaddr_un_ptr).sun_family as i32 == AF_UNIX {
                let sun_path_ptr = (*sockaddr_un_ptr).sun_path.as_mut_ptr();
                let path_len = strlen(sun_path_ptr);
                const LIND_ROOT_LEN: usize = LIND_ROOT.len();
        
                let new_path_len = path_len + LIND_ROOT_LEN;
        
                if new_path_len < 108 {
                    memmove(
                        sun_path_ptr.add(LIND_ROOT_LEN) as *mut libc::c_void,
                        sun_path_ptr as *const libc::c_void,
                        path_len,
                    );
        
                    libc::memcpy(
                        sun_path_ptr as *mut libc::c_void,
                        LIND_ROOT.as_ptr() as *const libc::c_void,
                        LIND_ROOT_LEN,
                    );

                    libc::memset(
                        sun_path_ptr.add(new_path_len) as *mut libc::c_void,
                        0,
                        108 - new_path_len,
                    );
                }
            }
        }
        (sockaddr_un_ptr, size_of::<libc::sockaddr_un>() as u32)
    };

    let finalsockaddr = finalsockaddr as *mut libc::sockaddr;

    let ret = unsafe { libc::bind(fd as i32, finalsockaddr, addrlen as u32) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "bind");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/listen.2.html
///
/// The Linux `listen()` syscall marks a socket as passive, indicating that it will be used to accept
/// incoming connection requests. This implementation converts the virtual file descriptor and backlog
/// value from the calling cage to their kernel-visible equivalents, and invokes the system `listen()` call.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor referring to the socket
///     - backlog_arg: Maximum number of pending connections in the socket’s listen queue
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
pub fn listen_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    backlog_arg: u64,
    backlog_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let backlog = sc_convert_sysarg_to_i32(backlog_arg, backlog_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "listen_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::listen(fd, backlog) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "listen");
    }
    ret
}



/// Reference to Linux: https://man7.org/linux/man-pages/man2/accept.2.html
///
/// The Linux `accept()` syscall extracts the first connection request on the queue of pending
/// connections for the listening socket, creates a new connected socket, and returns a new file descriptor
/// referring to that socket. In this implementation, we convert the virtual file descriptor to the host one,
/// and if provided, transform the sockaddr pointer for use inside the kernel. The returned host file
/// descriptor is then assigned a new virtual file descriptor.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor referring to the listening socket
///     - addr_arg: Optional pointer to a buffer that will receive the address of the connecting entity
///     - len_arg: not used in this implementation
///
/// Return:
///     - On success: New virtual file descriptor associated with the accepted socket
///     - On failure: A negative errno value indicating the syscall error
pub fn accept_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "accept_syscall", "Invalide Cage ID");
    }

    // Handle sockaddr conversion; if NULL, use empty pointer
    let (finalsockaddr, mut addrlen) = if addr.is_null() {
        (std::ptr::null::<libc::sockaddr_un>() as *mut libc::sockaddr_un, 0)
    } else {
        let mut sockaddr_un: sockaddr_un = sockaddr_un {
            sun_family: 0 as u16,
            sun_path: [0; 108],
        };

        let sockaddr_un_ptr: *mut sockaddr_un = &mut sockaddr_un;
        
        unsafe {
            // Copy user's sockaddr to local buffer
            std::ptr::copy_nonoverlapping(addr as *mut libc::sockaddr_un, sockaddr_un_ptr, 1);
            
            // If AF_UNIX socket, rewrite sun_path with LIND_ROOT prefix
            if (*sockaddr_un_ptr).sun_family as i32 == AF_UNIX {
                let sun_path_ptr = (*sockaddr_un_ptr).sun_path.as_mut_ptr();
                let path_len = strlen(sun_path_ptr);
                const LIND_ROOT_LEN: usize = LIND_ROOT.len();
        
                let new_path_len = path_len + LIND_ROOT_LEN;
        
                if new_path_len < 108 {
                    memmove(
                        sun_path_ptr.add(LIND_ROOT_LEN) as *mut libc::c_void,
                        sun_path_ptr as *const libc::c_void,
                        path_len,
                    );
        
                    libc::memcpy(
                        sun_path_ptr as *mut libc::c_void,
                        LIND_ROOT.as_ptr() as *const libc::c_void,
                        LIND_ROOT_LEN,
                    );

                    libc::memset(
                        sun_path_ptr.add(new_path_len) as *mut libc::c_void,
                        0,
                        108 - new_path_len,
                    );
                }
            }
        }
        (sockaddr_un_ptr, size_of::<libc::sockaddr_un>() as u32)
    };

    let finalsockaddr = finalsockaddr as *mut libc::sockaddr;

    let ret_kernelfd = unsafe { libc::accept(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret_kernelfd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "accept");
    }

    let ret_virtualfd = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ret_kernelfd as u64, false, 0).unwrap();
    
    ret_virtualfd as i32

}


/// Reference to Linux: https://man7.org/linux/man-pages/man2/setsockopt.2.html
///
/// The Linux `setsockopt()` syscall sets options for a socket. Options may exist at multiple protocol levels.
/// This implementation translates the virtual file descriptor and user-provided option values into host-space values
/// before applying the `setsockopt` syscall on the host kernel.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor representing the socket
///     - level_arg: Specifies the protocol level at which the option resides (e.g., SOL_SOCKET)
///     - optname_arg: Option name to be set (e.g., SO_REUSEADDR)
///     - optval_arg: Pointer to the option value
///     - optlen_arg: Size of the option value
///
/// Return:
///     - On success: 0
///     - On failure: A negative errno value indicating the syscall error
pub fn setsockopt_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    level_arg: u64,
    level_cageid: u64,
    optname_arg: u64,
    optname_cageid: u64,
    optval_arg: u64,
    optval_cageid: u64,
    optlen_arg: u64,
    optlen_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let level = sc_convert_sysarg_to_i32(level_arg, level_cageid, cageid);
    let optname = sc_convert_sysarg_to_i32(optname_arg, optname_cageid, cageid);
    let optval = sc_convert_addr_to_host(optval_arg, optval_cageid, cageid);
    let optlen = sc_convert_sysarg_to_u32(optlen_arg, optlen_cageid, cageid);

    if !(sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "setsockopt_syscall", "Invalide Cage ID");
    }
    let ret = unsafe { 
        libc::setsockopt(fd, level, optname, optval as *mut c_void, optlen)
    };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "setsockopt");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/send.2.html
///
/// The Linux `send()` syscall is used to transmit a message through a socket.
/// This implementation extracts the virtual file descriptor and buffer from the current cage,
/// then forwards the request to the host kernel with the provided flags.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor indicating the socket to send data on
///     - buf_arg: Pointer to the message buffer in user memory
///     - buflen_arg: Length of the message to be sent
///     - flags_arg: Bitmask of flags influencing message transmission behavior
///
/// Return:
///     - On success: Number of bytes sent
///     - On failure: A negative errno value indicating the syscall error
pub fn send_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = sc_convert_buf_to_host(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "send_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::send(fd as i32, buf as *const c_void, buflen, flags) as i32};
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "send");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/recv.2.html
///
/// The Linux `recv()` syscall is used to receive a message from a connected socket.
/// This implementation retrieves the virtual file descriptor and target buffer from the current cage,
/// and performs the message receive operation using the specified flags.
///
/// Input:
///     - cageid: current cageid
///     - fd_arg: Virtual file descriptor from which to receive data
///     - buf_arg: Pointer to the buffer in user memory to store received data
///     - buflen_arg: Size of the buffer to receive data into
///     - flags_arg: Flags controlling message reception behavior
///
/// Return:
///     - On success: Number of bytes received
///     - On failure: A negative errno value indicating the syscall error
pub fn recv_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = sc_convert_buf_to_host(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "recv_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::recv(fd, buf as *mut c_void, buflen, flags) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "recv");
    }
    ret
}