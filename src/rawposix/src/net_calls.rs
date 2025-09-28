use typemap::datatype_conversion::*;
use typemap::network_helpers::{convert_host_sockaddr, convert_sockpair, copy_out_sockaddr};
use typemap::cage_helpers::convert_fd_to_host;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::FDKIND_KERNEL;
use sysdefs::data::net_struct::SockAddr;

/// Reference to Linux: https://man7.org/linux/man-pages/man2/socket.2.html
///
/// The Linux `socket()` syscall creates an endpoint for communication and returns a file descriptor
/// for the newly created socket. This implementation wraps the syscall and registers the resulting
/// file descriptor in our virtual file descriptor table (`fdtables`) under the current cage.
///
/// The `fdtables` system manages per-cage file descriptors and tracks their lifecycle.
///
/// ## Input:
///     - cageid: current cageid
///     - domain_arg: communication domain (e.g., AF_INET, AF_UNIX)
///     - socktype_arg: socket type (e.g., SOCK_STREAM, SOCK_DGRAM)
///     - protocol_arg: protocol to be used (usually 0)
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "socket_syscall", "Invalid Cage ID");
    }

    let kernel_fd = unsafe { libc::socket(domain, socktype, protocol) };
       
    if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "socket");
    }

    // We need to register this new kernel fd in fdtables
    // Check if `SOCK_CLOEXEC` flag is set
    let cloexec = (socktype & SOCK_CLOEXEC) != 0;

    // Register the kernel fd in fdtables with or without cloexec
    // Note:
    // `SOCK_NONBLOCK` is part of the kernel's "open file description" state
    // (equivalent to `O_NONBLOCK`). Since our virtual FD maps directly to a
    // host kernel FD (`FDKIND_KERNEL`), we simply defer to the kernel as the
    // source of truth and do not duplicate this flag in `fdtables::optionalinfo`.
    fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, cloexec, 0)
        .unwrap() as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/connect.2.html
///
/// The Linux `connect()` syscall initiates a connection on a socket referred to by a file
/// descriptor. This wrapper:
///   1) Resolves the caller’s virtual FD (per-cage) to a host kernel FD,
///   2) Translates the user-provided sockaddr pointer (guest/cage VA) to a host pointer,
///   3) Normalizes the sockaddr content/length (e.g., UNIX domain path rules),
///   4) Invokes `libc::connect` on the host,
///   5) Converts any errno into our error return convention.
///
/// ## Inputs:
///   - `cageid`:         Current cage id (selects the per-cage FD table and address space).
///   - `fd_arg`:         Virtual file descriptor in the caller cage.
///   - `fd_cageid`:      Cage id that `fd_arg` belongs to (validated when `secure` is enabled).
///   - `addr_arg`:       Guest virtual address of a sockaddr buffer supplied by the caller.
///   - `addr_cageid`:    Cage id that `addr_arg` belongs to (validated when `secure` is enabled).
///   - `arg3..arg6`:     Unused here; must be empty/zero. When the `secure` feature is enabled,
///                       non-empty values cause the call to fail with EFAULT (cage id misuse).
///
/// ## Returns:
///   - On success: `0`
///   - On failure: negative errno value converted via `handle_errno`
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
        &&sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "connect_syscall", "Invalid Cage ID");
    }
    
    let (finalsockaddr, addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);

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
/// ## Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor to be bound
///     - addr_arg: pointer to a `sockaddr_un` structure containing the local address
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "bind_syscall", "Invalid Cage ID");
    }
    
    let (finalsockaddr, addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);

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
/// ## Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor referring to the socket
///     - backlog_arg: maximum number of pending connections in the socket’s listen queue
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "listen_syscall", "Invalid Cage ID");
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
/// ## Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor referring to the listening socket
///     - addr_arg: optional pointer to a buffer that will receive the address of the connecting entity
///     - len_arg: not used in this implementation
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "accept_syscall", "Invalid Cage ID");
    }

    let (finalsockaddr, mut addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);

    let ret_kernelfd = unsafe { libc::accept(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret_kernelfd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "accept");
    }

    // We need to register this new kernel fd in fdtables
    let ret_virtualfd = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ret_kernelfd as u64, false, 0).unwrap();
    
    ret_virtualfd as i32

}


/// Reference to Linux: https://man7.org/linux/man-pages/man2/setsockopt.2.html
///
/// The Linux `setsockopt()` syscall sets options for a socket. Options may exist at multiple protocol levels.
/// This implementation translates the virtual file descriptor and user-provided option values into host-space values
/// before applying the `setsockopt` syscall on the host kernel.
///
/// ## Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor representing the socket
///     - level_arg: specifies the protocol level at which the option resides (e.g., SOL_SOCKET)
///     - optname_arg: option name to be set (e.g., SO_REUSEADDR)
///     - optval_arg: pointer to the option value
///     - optlen_arg: size of the option value
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "setsockopt_syscall", "Invalid Cage ID");
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/shutdown.2.html
///
/// The Linux `shutdown()` syscall disables sends and/or receives on a socket.
/// This implementation resolves the given virtual file descriptor to the host kernel
/// file descriptor, then performs the shutdown operation in the host kernel.
///
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the socket
///     - how_arg: specifies the type of shutdown (e.g., SHUT_RD, SHUT_WR, SHUT_RDWR)
///
/// ## Return:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn shutdown_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    how_arg: u64,
    how_cageid: u64,
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
    let how = sc_convert_sysarg_to_i32(how_arg, how_cageid, cageid);

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "shutdown_syscall", "Invalid Cage ID");
    }

    let ret = unsafe { libc::shutdown(fd, how) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "shutdown");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getsockname.2.html
///
/// The Linux `getsockname()` syscall retrieves the current address to which the socket
/// is bound.  
/// This implementation resolves the virtual file descriptor to the host kernel file descriptor,
/// converts the user-space sockaddr structure into its host representation, and invokes
/// the host kernel `getsockname()`.
///
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the socket
///     - addr_arg: pointer to a buffer in user space where the address will be stored
///
/// ## Return:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn getsockname_syscall(
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
    
    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getsockname_syscall", "Invalid Cage ID");
    }
    
    let (finalsockaddr, addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);

    let ret = unsafe { libc::getsockname(fd as i32, finalsockaddr, addrlen as *mut u32) };

    if ret < 0  {
        let errno = get_errno();
        return handle_errno(errno, "getsockname");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/recv.2.html
///
/// The Linux `recv()` syscall is used to receive a message from a connected socket.
/// This implementation retrieves the virtual file descriptor and target buffer from the current cage,
/// and performs the message receive operation using the specified flags.
///
/// ## Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor from which to receive data
///     - buf_arg: pointer to the buffer in user memory to store received data
///     - buflen_arg: size of the buffer to receive data into
///     - flags_arg: flags controlling message reception behavior
///
/// ## Return:
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
    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "recv_syscall", "Invalid Cage ID");
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
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor representing the socket
///     - buf_arg: pointer to the message buffer in user space
///     - buflen_arg: length of the message to send
///     - flag_arg: flags influencing message transmission behavior
///     - sockaddr_arg: pointer to the destination socket address
///     - addrlen_arg: size of the destination address structure
///
/// ## Return:
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

    let (finalsockaddr, addrlen) = convert_host_sockaddr(sockaddr, sockaddr_cageid, cageid);

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
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor representing the socket
///     - buf_arg: pointer to the buffer in user space to store received data
///     - buflen_arg: size of the buffer
///     - flag_arg: Flags controlling message reception behavior
///     - nullity1_arg(src_addr): pointer to the source address structure or null
///     - nullity2_arg(addrlen): pointer to the source address length or null
///
/// ## Return:
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

    // true means user passed NULL for that pointer
    let nullity1 = sc_convert_arg_nullity(nullity1_arg, nullity1_cageid, cageid);
    let nullity2 = sc_convert_arg_nullity(nullity2_arg,nullity2_cageid, cageid);

    // Case 1: both NULL → caller doesn’t want peer address
    if nullity1 && nullity2 {
        let (finalsockaddr, mut addrlen) = convert_host_sockaddr(ptr::null_mut(), nullity1_cageid, cageid);
        let ret = unsafe { libc::recvfrom(fd, buf as *mut c_void, buflen, flag, finalsockaddr, &mut addrlen as *mut u32) as i32 };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }
    }
    // Case 2: both non-NULL → caller wants src_addr + addrlen filled
    else if !(nullity1 || nullity2) {
        let mut newsockaddr = SockAddr::new_ipv4();
        let ptr = &mut newsockaddr as *mut SockAddr as *mut u8;
        let (finalsockaddr, mut addrlen) = convert_host_sockaddr(ptr, nullity1_cageid, cageid); 
        let ret = unsafe { libc::recvfrom(fd, buf as *mut c_void, buflen, flag, finalsockaddr, &mut addrlen as *mut u32) as i32 };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }

        // Copy peer address back to user’s src_addr / addrlen
        if ret >= 0 {
            copy_out_sockaddr(
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
/// ## Input:
///     - cageid: identifier of the current cage
///     - name_arg: pointer to the buffer in user space to store the hostname
///     - len_arg: size of the buffer
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "gethostname_syscall", "Invalid Cage ID");
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
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the socket
///     - level_arg: protocol level at which the option resides
///     - optname_arg: name of the option to retrieve
///     - optval_arg: pointer to a buffer to store the option value
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg5, arg5_cageid)
    &&sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getsockopt_syscall", "Invalid Cage ID");
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
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the connected socket
///     - addr_arg: pointer to a buffer in user space to store the peer address
///
/// ## Return:
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

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getpeername_syscall", "Invalid Cage ID");
    }

    let (finalsockaddr, mut addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);
    let ret = unsafe { libc::getpeername(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "getpeername");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/socketpair.2.html
///
/// The Linux `socketpair()` syscall creates a pair of connected sockets.
/// This implementation creates the socket pair in the host kernel and assigns virtual file descriptors
/// to the resulting sockets within the current cage.
///
/// ## Input:
///     - cageid: identifier of the current cage
///     - domain_arg: communication domain (e.g., AF_UNIX)
///     - type_arg: communication semantics (e.g., SOCK_STREAM)
///     - protocol_arg: protocol to be used
///     - virtual_socket_vector_arg: pointer to a `SockPair` structure in user space to receive the result
///nullity1/2
/// ## Return:
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
    let typ = sc_convert_sysarg_to_i32(type_arg, type_cageid, cageid);
    let protocol = sc_convert_sysarg_to_i32(protocol_arg, protocol_cageid, cageid);
    let virtual_socket_vector = convert_sockpair(virtual_socket_vector_arg, virtual_socket_vector_cageid, cageid).unwrap();
    
    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "socketpair_syscall", "Invalid Cage ID");
    }

    let mut kernel_socket_vector: [i32; 2] = [0, 0];

    let ret = unsafe { libc::socketpair(domain, typ, protocol, kernel_socket_vector.as_mut_ptr()) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "sockpair");
    }

    let ksv_1 = kernel_socket_vector[0];
    let ksv_2 = kernel_socket_vector[1];

    // We need to register this new kernel fd in fdtables
    // Check if `SOCK_CLOEXEC` flag is set
    let cloexec = (typ & SOCK_CLOEXEC) != 0;

    // Register the kernel fd in fdtables with or without cloexec
    // Note:
    // `SOCK_NONBLOCK` is part of the kernel's "open file description" state
    // (equivalent to `O_NONBLOCK`). Since our virtual FD maps directly to a
    // host kernel FD (`FDKIND_KERNEL`), we simply defer to the kernel as the
    // source of truth and do not duplicate this flag in `fdtables::optionalinfo`.
    let vsv_1 = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ksv_1 as u64, cloexec, 0).unwrap();
    let vsv_2 = fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ksv_2 as u64, cloexec, 0).unwrap();

    // Update virtual socketpair struct
    virtual_socket_vector.sock1 = vsv_1 as i32;
    virtual_socket_vector.sock2 = vsv_2 as i32;
    return 0;
}
