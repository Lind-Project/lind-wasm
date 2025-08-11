use typemap::syscall_type_conversion::*;
use typemap::fs_type_conversion::*;
use typemap::network_type_conversion::*;
use fdtables;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use libc::*;
use typemap::CStr;
use std::io;
use std::io::Write;
use std::ptr;
use std::collections::{HashMap, HashSet};
use sysdefs::*;
use lazy_static::lazy_static;
use parking_lot::Mutex;

lazy_static! {
    // A hashmap used to store epoll mapping relationships
    // <virtual_epfd <kernel_fd, virtual_fd>>
    static ref REAL_EPOLL_MAP: Mutex<HashMap<u64, HashMap<i32, u64>>> = Mutex::new(HashMap::new());
}

const FDKIND_KERNEL: u32 = 0;

/// Reference to Linux: https://man7.org/linux/man-pages/man2/socket.2.html
///
/// The Linux `socket()` syscall creates an endpoint for communication and returns a file descriptor
/// for the newly created socket. This implementation wraps the syscall and registers the resulting
/// file descriptor in our virtual file descriptor table (`fdtables`) under the current cage.
///
/// The `fdtables` system manages per-cage file descriptors and tracks their lifecycle.
///
/// Input:
///     - cageid: current cageid
///     - domain_arg: communication domain (e.g., AF_INET, AF_UNIX)
///     - socktype_arg: socket type (e.g., SOCK_STREAM, SOCK_DGRAM)
///     - protocol_arg: protocol to be used (usually 0)
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
///     - fd_arg: virtual file descriptor for the socket to be connected
///     - addr_arg: pointer to a `sockaddr_un` structure containing the target address
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
    
    let (finalsockaddr, addrlen) = sc_convert_host_sockaddr(addr,addr_cageid, cageid);

    let ret = unsafe { libc::connect(fd, finalsockaddr, addrlen) };
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
///     - fd_arg: virtual file descriptor to be bound
///     - addr_arg: pointer to a `sockaddr_un` structure containing the local address
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
    
    let (finalsockaddr, addrlen) = sc_convert_host_sockaddr(addr, addr_cageid, cageid);

    let ret = unsafe { libc::bind(fd, finalsockaddr, addrlen) };
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
///     - fd_arg: virtual file descriptor referring to the socket
///     - backlog_arg: maximum number of pending connections in the socket’s listen queue
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
///     - fd_arg: virtual file descriptor referring to the listening socket
///     - addr_arg: optional pointer to a buffer that will receive the address of the connecting entity
///     - len_arg: not used in this implementation
///
/// Return:
///     - On success: new virtual file descriptor associated with the accepted socket
///     - On failure: a negative errno value indicating the syscall error
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

    let (finalsockaddr, mut addrlen) = sc_convert_host_sockaddr(addr, addr_cageid, cageid);

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
///     - fd_arg: virtual file descriptor representing the socket
///     - level_arg: specifies the protocol level at which the option resides (e.g., SOL_SOCKET)
///     - optname_arg: option name to be set (e.g., SO_REUSEADDR)
///     - optval_arg: pointer to the option value
///     - optlen_arg: size of the option value
///
/// Return:
///     - On success: 0
///     - On failure: a negative errno value indicating the syscall error
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
///     - fd_arg: virtual file descriptor indicating the socket to send data on
///     - buf_arg: pointer to the message buffer in user memory
///     - buflen_arg: length of the message to be sent
///     - flags_arg: bitmask of flags influencing message transmission behavior
///
/// Return:
///     - On success: number of bytes sent
///     - On failure: a negative errno value indicating the syscall error
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
///     - fd_arg: virtual file descriptor from which to receive data
///     - buf_arg: pointer to the buffer in user memory to store received data
///     - buflen_arg: size of the buffer to receive data into
///     - flags_arg: flags controlling message reception behavior
///
/// Return:
///     - On success: number of bytes received
///     - On failure: a negative errno value indicating the syscall error
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/sendto.2.html
///
/// The Linux `sendto()` syscall is used to transmit a message to a specific address using a socket.
/// This implementation retrieves the virtual file descriptor, buffer, and target socket address
/// from the current cage, then invokes the host kernel's `sendto()` call.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor representing the socket
///     - buf_arg: pointer to the message buffer in user space
///     - buflen_arg: length of the message to send
///     - flag_arg: flags influencing message transmission behavior
///     - sockaddr_arg: pointer to the destination socket address
///     - addrlen_arg: size of the destination address structure
///
/// Returns:
///     - On success: number of bytes sent
///     - On failure: negative errno indicating the error
pub fn sendto_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flag_arg: u64,
    flag_cageid: u64,
    sockaddr_arg: u64,
    sockaddr_cageid: u64,
    addrlen_arg: u64,
    addrlen_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = sc_convert_addr_to_host(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flag = sc_convert_sysarg_to_i32(flag_arg, flag_cageid, cageid);
    let sockaddr = sc_convert_addr_to_host(sockaddr_arg, sockaddr_cageid, cageid);
    let addrlen = sc_convert_sysarg_to_u32(addrlen_arg, addrlen_cageid, cageid);

    let (finalsockaddr, addrlen) = sc_convert_host_sockaddr(sockaddr, sockaddr_cageid, cageid);

    let ret = unsafe {
        libc::sendto(
            fd,
            buf as *const c_void,
            buflen,
            flag,
            finalsockaddr,
            addrlen,
        ) as i32
    };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "sendto");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/recvfrom.2.html
///
/// The Linux `recvfrom()` syscall is used to receive a message from a socket,
/// optionally storing the source address of the sender.
/// This implementation retrieves the virtual file descriptor and buffer from the current cage,
/// and optionally copies back the source address to user space.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor representing the socket
///     - buf_arg: pointer to the buffer in user space to store received data
///     - buflen_arg: size of the buffer
///     - flag_arg: Flags controlling message reception behavior
///     - nullity1_arg: pointer to the source address structure or null
///     - nullity2_arg: pointer to the source address length or null
///
/// Returns:
///     - On success: number of bytes received
///     - On failure: negative errno indicating the error
pub fn recvfrom_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flag_arg: u64,
    flag_cageid: u64,
    nullity1_arg: u64,
    nullity1_cageid: u64,
    nullity2_arg: u64,
    nullity2_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = sc_convert_addr_to_host(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flag = sc_convert_sysarg_to_i32(flag_arg, flag_cageid, cageid);

    let nullity1 = sc_convert_arg_nullity(nullity1_arg, nullity1_cageid, cageid);
    let nullity2 = sc_convert_arg_nullity(nullity2_arg,nullity2_cageid, cageid);

    if nullity1 && nullity2 {
        let (finalsockaddr, mut addrlen) = sc_convert_host_sockaddr(ptr::null_mut(), nullity1_cageid, cageid);
        let ret = unsafe { libc::recvfrom(fd, buf as *mut c_void, buflen, flag, finalsockaddr, &mut addrlen as *mut u32) as i32 };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }
    }

    else if !(nullity1 || nullity2) {
        let mut newsockaddr = SockAddr::new_ipv4();
        let ptr = &mut newsockaddr as *mut SockAddr as *mut u8;
        let (finalsockaddr, mut addrlen) = sc_convert_host_sockaddr(ptr, nullity1_cageid, cageid); 
        let ret = unsafe { libc::recvfrom(fd, buf as *mut c_void, buflen, flag, finalsockaddr, &mut addrlen as *mut u32) as i32 };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }

        if ret >= 0 {
            sc_convert_copy_out_sockaddr(
                sc_convert_uaddr_to_host(nullity1_arg, nullity1_cageid, cageid),
                sc_convert_uaddr_to_host(nullity2_arg, nullity2_cageid, cageid) as u64,
                newsockaddr.sun_family,
            );
        }
    }

    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/gethostname.2.html
///
/// The Linux `gethostname()` syscall returns the current host name of the system.
/// This implementation retrieves the destination buffer and length from the current cage,
/// and stores the host name into user space.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - name_arg: pointer to the buffer in user space to store the hostname
///     - len_arg: size of the buffer
///
/// Returns:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn gethostname_syscall(
    cageid: u64,
    name_arg: u64,
    name_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let name = sc_convert_addr_to_host(name_arg, name_cageid, cageid);
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "gethostname_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::gethostname(name as *mut i8, len) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "gethostname");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getsockopt.2.html
///
/// The Linux `getsockopt()` syscall retrieves the value of a socket option.
/// This implementation retrieves the virtual file descriptor, option level, and option name
/// from the current cage, and writes the result to the provided user-space buffer.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the socket
///     - level_arg: protocol level at which the option resides
///     - optname_arg: name of the option to retrieve
///     - optval_arg: pointer to a buffer to store the option value
///
/// Returns:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn getsockopt_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    level_arg: u64,
    level_cageid: u64,
    optname_arg: u64,
    optname_cageid: u64,
    optval_arg: u64,
    optval_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32{
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let level = sc_convert_sysarg_to_i32(level_arg, level_cageid, cageid);
    let optname = sc_convert_sysarg_to_i32(optname_arg, optname_cageid, cageid);
    let optval = sc_convert_sysarg_to_i32_ref(optval_arg, optval_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid)
    &&sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getsockopt_syscall", "Invalide Cage ID");
    }

    let mut optlen: socklen_t = 4;

    let ret = unsafe {
        libc::getsockopt(
            fd,
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getpeername.2.html
///
/// The Linux `getpeername()` syscall retrieves the address of the peer connected to a socket.
/// This implementation obtains the socket file descriptor and address buffer from the current cage,
/// then invokes the host kernel's `getpeername()` and writes the result to user space.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the connected socket
///     - addr_arg: pointer to a buffer in user space to store the peer address
///
/// Returns:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn getpeername_syscall(
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
        return syscall_error(Errno::EFAULT, "getpeername_syscall", "Invalide Cage ID");
    }

    let (finalsockaddr, mut addrlen) = sc_convert_host_sockaddr(addr, addr_cageid, cageid);
    let ret = unsafe { libc::getpeername(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret < 0 {
        let err = unsafe {
            libc::__errno_location()
        };
        let err_str = unsafe {
            libc::strerror(*err)
        };
        let err_msg = unsafe {
            CStr::from_ptr(err_str).to_string_lossy().into_owned()
        };
        println!("[getpeername] Error message: {:?}", err_msg);
        
        let errno = get_errno();
        println!("[getpeername] Errno: {:?}", errno);
        io::stdout().flush().unwrap();
        return handle_errno(errno, "getpeername");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/poll.2.html
///
/// The Linux `poll()` syscall waits for events on multiple file descriptors.
/// This implementation converts a slice of user-space poll structures from the current cage,
/// invokes the host kernel's `poll()` call, and copies the result back to user space.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - addr_arg: pointer to the array of `PollStruct` in user space
///     - nfds_arg: number of file descriptors in the array
///     - timeout_arg: timeout in milliseconds, or -1 to block indefinitely
///
/// Returns:
///     - On success: number of file descriptors with events
///     - On failure: negative errno indicating the error
pub fn poll_syscall(
    cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    nfds_arg: u64,
    nfds_cageid: u64,
    timeout_arg: u64,
    timeout_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let addr = sc_convert_uaddr_to_host(addr_arg, addr_cageid, cageid);
    let nfds = sc_convert_sysarg_to_usize(nfds_arg,nfds_cageid, cageid);
    let pollfds = sc_convert_pollstruct_slice(addr, addr_cageid, cageid, nfds).unwrap();
    let timeout = sc_convert_sysarg_to_i32(timeout_arg, timeout_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "poll_syscall", "Invalide Cage ID");
    }

    let mut real_fd = virtual_to_real_poll(cageid, pollfds);
    let ret = unsafe { libc::poll(real_fd.as_mut_ptr(), nfds as u64, timeout) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "poll");
    }
    for (i, libcpoll) in real_fd.iter().enumerate() {
        if let Some(rposix_poll) = pollfds.get_mut(i) {
                rposix_poll.revents = libcpoll.revents;
        }
    }
        
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_create.2.html
///
/// The Linux `epoll_create()` syscall creates a new epoll instance and returns a file descriptor referring to it.
/// This implementation retrieves the size hint from the current cage and allocates a virtual file descriptor for the epoll instance.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - size_arg: size hint for the number of file descriptors to be monitored (ignored in modern kernels)
///
/// Returns:
///     - On success: virtual file descriptor for the new epoll instance
///     - On failure: negative errno indicating the error
pub fn epoll_create_syscall(
    cageid: u64,
    size_arg: u64,
    size_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let size = sc_convert_sysarg_to_i32(size_arg, size_cageid, cageid);
    
    if !(sc_unusedarg(arg2, arg2_cageid)
    &&sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "epoll_create_syscall", "Invalide Cage ID");
    } 

    let kernel_fd = unsafe { libc::epoll_create(size) };
        
        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "epoll_create");
        }

        let virtual_epfd = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, false, 0).unwrap();

        virtual_epfd as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_ctl.2.html
///
/// The Linux `epoll_ctl()` syscall performs control operations on an epoll instance,
/// such as adding, modifying, or removing file descriptors.
/// This implementation retrieves the epoll instance, target file descriptor,
/// and event configuration from the current cage, and synchronizes the internal mapping.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - epfd_arg: virtual file descriptor of the epoll instance
///     - op_arg: operation to be performed (e.g., EPOLL_CTL_ADD, EPOLL_CTL_MOD, EPOLL_CTL_DEL)
///     - fd_arg: virtual file descriptor to operate on
///     - epollevent_arg: pointer to an `EpollEvent` structure in user space
///
/// Returns:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn epoll_ctl_syscall(
    cageid: u64,
    epfd_arg: u64,
    epfd_cageid: u64,
    op_arg: u64,
    op_cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    epollevent_arg: u64,
    epollevent_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let epfd = convert_fd_to_host(epfd_arg, epfd_cageid, cageid);
    let op = sc_convert_sysarg_to_i32(op_arg, op_cageid, cageid);
    let vfd = sc_convert_sysarg_to_i32(fd_arg, fd_cageid, cageid);
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let epollevent = sc_convert_epollevent(epollevent_arg, epollevent_cageid, cageid).unwrap();

    println!{}
    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "epoll_ctl_syscall", "Invalide Cage ID");
    }
    
    let event = epollevent.events;
    
    let mut epoll_event = epoll_event {
        events: event,
        u64: fd as u64,
    };
        
    let ret = unsafe { libc::epoll_ctl(epfd as i32, op, fd as i32, &mut epoll_event) };
        
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "epoll_ctl");
    }

    if op == libc::EPOLL_CTL_DEL {
        let mut epollmapping = REAL_EPOLL_MAP.lock();
        if let Some(fdmap) = epollmapping.get_mut(&(epfd as u64)) {
            if fdmap.remove(&(fd as i32)).is_some() {
                if fdmap.is_empty() {
                    epollmapping.remove(&(epfd as u64));
                }
                return ret;
            }
        }
    } else {
        let mut epollmapping = REAL_EPOLL_MAP.lock();
        epollmapping.entry(epfd as u64).or_insert_with(HashMap::new).insert(fd as i32, vfd as u64);
        return ret;
    }

    -1
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_wait.2.html
///
/// The Linux `epoll_wait()` syscall waits for events on the epoll instance referred to by a file descriptor.
/// This implementation retrieves the virtual epoll descriptor, result buffer, and timeout from the current cage,
/// then invokes the host kernel's `epoll_wait()` and maps the results back to virtual file descriptors.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - epfd_arg: virtual file descriptor of the epoll instance
///     - events_arg: pointer to a buffer to receive triggered events
///     - maxevents_arg: maximum number of events to retrieve
///     - timeout_arg: timeout in milliseconds, or -1 to block indefinitely
///
/// Returns:
///     - On success: number of file descriptors with events
///     - On failure: negative errno indicating the error
pub fn epoll_wait_syscall(
    cageid: u64,
    epfd_arg: u64,
    epfd_cageid: u64,
    events_arg: u64,
    events_cageid: u64,
    maxevents_arg: u64,
    maxevents_cageid: u64,
    timeout_arg: u64,
    timeout_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let epfd = sc_convert_sysarg_to_i32(epfd_arg, epfd_cageid, cageid);
    let maxevents = sc_convert_sysarg_to_i32(maxevents_arg, maxevents_cageid, cageid);
    let events = sc_convert_epollevent_slice(events_arg, events_cageid, cageid, maxevents).unwrap();
    let timeout = sc_convert_sysarg_to_i32(timeout_arg, timeout_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "epoll_wait_syscall", "Invalide Cage ID");
    }

    
    let k_epfd = fdtables::translate_virtual_fd(cageid, epfd as u64);
    if k_epfd.is_err() {
        return syscall_error(Errno::EBADF, "epoll_wait", "Bad File Descriptor");
    }
    let kernel_epfd = k_epfd.unwrap();
    
    let mut kernel_events: Vec<epoll_event> = Vec::with_capacity(maxevents as usize);

    kernel_events.push(
        epoll_event {
            events: 0,
            u64: 0,
        }
    );

    let ret = unsafe { libc::epoll_wait(kernel_epfd.underfd as i32, kernel_events.as_mut_ptr(), maxevents, timeout as i32) };
    
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "epoll_wait");
    }

    for i in 0..ret as usize {

        let ret_kernelfd = kernel_events[i].u64;
        let epollmapping = REAL_EPOLL_MAP.lock();
        let ret_virtualfd = epollmapping.get(&(epfd_arg as u64)).and_then(|kernel_map| kernel_map.get(&(ret_kernelfd as i32)).copied());

        events[i].fd = ret_virtualfd.unwrap() as i32;
        events[i].events = kernel_events[i].events;
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/socketpair.2.html
///
/// The Linux `socketpair()` syscall creates a pair of connected sockets.
/// This implementation creates the socket pair in the host kernel and assigns virtual file descriptors
/// to the resulting sockets within the current cage.
///
/// Parameters:
///     - cageid: identifier of the current cage
///     - domain_arg: communication domain (e.g., AF_UNIX)
///     - type_arg: communication semantics (e.g., SOCK_STREAM)
///     - protocol_arg: protocol to be used
///     - virtual_socket_vector_arg: pointer to a `SockPair` structure in user space to receive the result
///
/// Returns:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn socketpair_syscall(
    cageid: u64,
    domain_arg: u64,
    domain_cageid: u64,
    type_arg: u64,
    type_cageid: u64,
    protocol_arg: u64,
    protocol_cageid: u64,
    virtual_socket_vector_arg: u64,
    virtual_socket_vector_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let domain = sc_convert_sysarg_to_i32(domain_arg, domain_cageid, cageid);
    let type_ = sc_convert_sysarg_to_i32(type_arg, type_cageid, cageid);
    let protocol = sc_convert_sysarg_to_i32(protocol_arg, protocol_cageid, cageid);
    let virtual_socket_vector = sc_convert_sockpair(virtual_socket_vector_arg, virtual_socket_vector_cageid, cageid).unwrap();
    
    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "socketpair_syscall", "Invalide Cage ID");
    }

    let mut kernel_socket_vector: [i32; 2] = [0, 0];

    let ret = unsafe { libc::socketpair(domain, type_, protocol, kernel_socket_vector.as_mut_ptr()) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "sockpair");
    }

    let ksv_1 = kernel_socket_vector[0];
    let ksv_2 = kernel_socket_vector[1];
    let vsv_1 = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ksv_1 as u64, false, 0).unwrap();
    let vsv_2 = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ksv_2 as u64, false, 0).unwrap();
    virtual_socket_vector.sock1 = vsv_1 as i32;
    virtual_socket_vector.sock2 = vsv_2 as i32;
    return 0;
}

pub fn virtual_to_real_poll(cageid: u64, virtual_poll: &mut [PollStruct]) -> Vec<pollfd> {

    let mut real_fds = Vec::with_capacity(virtual_poll.len());

    for vfd in &mut *virtual_poll {

        let rfd = fdtables::translate_virtual_fd(cageid, vfd.fd as u64).unwrap();
        let real_fd = rfd.underfd;
        let kernel_poll = pollfd {
            fd: real_fd as i32,
            events: vfd.events,
            revents: vfd.revents,
        };
        real_fds.push(kernel_poll);
    }

    real_fds
}
