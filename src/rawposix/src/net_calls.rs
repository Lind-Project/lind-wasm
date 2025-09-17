use typemap::datatype_conversion;
use fdtables;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use libc::*;
use typemap::CStr;
use std::io::Write;
use std::ptr;
use sysdefs::*;
use lazy_static::lazy_static;

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
