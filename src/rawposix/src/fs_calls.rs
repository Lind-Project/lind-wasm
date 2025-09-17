use libc::c_void;
use typemap::datatype_conversion::*;
use typemap::path_conversion::*;
use sysdefs::constants::err_const::{syscall_error, Errno, get_errno, handle_errno};
use sysdefs::constants::fs_const::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO, O_CLOEXEC, FDKIND_KERNEL, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE, PAGESHIFT, PAGESIZE, MAXFD};
use sysdefs::constants::sys_const::{DEFAULT_UID, DEFAULT_GID};
use typemap::cage_helpers::*;
use cage::{round_up_page, get_cage, HEAP_ENTRY_INDEX, MemoryBackingType, VmmapOps};
use fdtables;
use std::collections::{HashMap, HashSet};
use libc::{pollfd, fd_set, timeval};
use std::time::Instant;

/// Helper function for close_syscall
/// 
/// Lind-WASM is running as same Linux-Process from host kernel perspective, so standard IO stream fds 
/// shouldn't be closed in Lind-WASM execution, which preventing issues where other threads might 
/// reassign these ds, causing unintended behavior or errors. 
/// 
/// This function is registered in `fdtables` when creating the cage
pub fn kernel_close(fdentry: fdtables::FDTableEntry, _count: u64) {
    let kernel_fd = fdentry.underfd as i32;

    if kernel_fd == STDIN_FILENO || kernel_fd == STDOUT_FILENO || kernel_fd == STDERR_FILENO {
        return;
    }

    let ret = unsafe { libc::close(fdentry.underfd as i32) };
    if ret < 0 {
        let errno = get_errno();
        panic!("kernel_close failed with errno: {:?}", errno);
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/open.2.html
///
/// Linux `open()` syscall will open a file descriptor and set file status and permissions according to user needs. Since we
/// implement a file descriptor management subsystem (called `fdtables`), so we need to open a new virtual fd
/// after getting the kernel fd. `fdtables` currently only manage when a fd should be closed after open, so
/// then we need to set `O_CLOEXEC` flags according to input.
///
/// ## Arguments:
///     This call will only have one cageid indicates current cage, and three regular arguments same with Linux
///     - cageid: current cage
///     - path_arg: This argument points to a pathname naming the file. User's perspective.
///     - oflag_arg: This argument contains the file status flags and file access modes which will be alloted to
///                 the open file description. The flags are combined together using a bitwise-inclusive-OR and the
///                 result is passed as an argument to the function. We need to check if `O_CLOEXEC` has been set.
///     - mode_arg: This represents the permission of the newly created file. Directly passing to kernel.
/// 
/// ## Returns:
/// same with man page
pub fn open_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    oflag_arg: u64,
    oflag_cageid: u64,
    mode_arg: u64,
    mode_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    // Note the cageid here isn't really relevant because the argument is pass-by-value.
    // But it could be checked to ensure it's not set to something unexpected.
    let oflag = sc_convert_sysarg_to_i32(oflag_arg, oflag_cageid, cageid);
    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "open_syscall", "Invalide Cage ID");
    }

    // Get the kernel fd first
    let kernel_fd = unsafe { libc::open(path.as_ptr(), oflag, mode) };

    if kernel_fd < 0 {
        return handle_errno(get_errno(), "open_syscall");
    }

    // Check if `O_CLOEXEC` has been est
    let should_cloexec = (oflag & fs_const::O_CLOEXEC) != 0;

    // Mapping a new virtual fd and set `O_CLOEXEC` flag
    match fdtables::get_unused_vfd_arg(
        cageid,
        fs_const::FDKIND_KERNEL,
        kernel_fd as u64,
        should_cloexec,
        0,
    ) {
        Ok(vfd_arg) => vfd_arg as i32,
        Err(_) => syscall_error(Errno::EMFILE, "open_syscall", "Too many files opened"),
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/read.2.html
///
/// Linux `read()` syscall attempts to read up to a specified number of bytes from a file descriptor into a buffer.
/// Since we implement a file descriptor management subsystem (called `fdtables`), we first translate the virtual file
/// descriptor into the corresponding kernel file descriptor before invoking the kernel's `libc::read()` function.
///
/// ## Arguments:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier.
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment.
///     - buf_arg: pointer to a buffer where the read data will be stored (user's perspective).
///     - count_arg: the maximum number of bytes to read from the file descriptor.
pub fn read_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    count_arg: u64,
    count_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Convert the virtual fd to the underlying kernel file descriptor.
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    if kernel_fd == -1 {
        return syscall_error(Errno::EFAULT, "read", "Invalid Cage ID");
    } else if kernel_fd == -9 {
        return syscall_error(Errno::EBADF, "read", "Bad File Descriptor");
    }

    // Convert the user buffer and count.
    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    if buf.is_null() {
        return syscall_error(Errno::EFAULT, "read", "Buffer is null");
    }

    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
         && sc_unusedarg(arg5, arg5_cageid)
         && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "read", "Invalid Cage ID");
    }

    // Early return if count is zero.
    if count == 0 {
        return 0;
    }

    // Call the underlying libc read.
    let ret = unsafe { libc::read(kernel_fd, buf as *mut c_void, count) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "read");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/close.2.html
///
/// Linux `close()` syscall closes a file descriptor. In our implementation, we use a file descriptor management
/// subsystem (called `fdtables`) to handle virtual file descriptors. This syscall removes the virtual file
/// descriptor from the subsystem, and if necessary, closes the underlying kernel file descriptor.
///
/// ## Arguments:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier.
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment to be closed.
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused.
pub fn close_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64, 
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
    if !(sc_unusedarg(arg3, arg3_cageid)
         && sc_unusedarg(arg4, arg4_cageid)
         && sc_unusedarg(arg5, arg5_cageid)
         && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "close", "Invalid Cage ID");
    }

    match fdtables::close_virtualfd(cageid, vfd_arg) {
        Ok(()) => 0,
        Err(e) => {
            if e == Errno::EBADFD as u64 {
                syscall_error(Errno::EBADF, "close", "Bad File Descriptor")
            } else if e == Errno::EINTR as u64 {
                syscall_error(Errno::EINTR, "close", "Interrupted system call")
            } else {
                syscall_error(Errno::EIO, "close", "I/O error")
            }
        }
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/mkdir.2.html
///
/// Linux `mkdir()` syscall creates a new directory named by the path name pointed to by a path as the input parameter
/// in the function. Since path seen by user is different from actual path on host, we need to convert the path first.
/// RawPOSIX doesn't have any other operations, so all operations will be handled by host. RawPOSIX does error handling
/// for this syscall.
///
/// ## Arguments:
///     - cageid: current cageid
///     - path_arg: This argument points to a pathname naming the file. User's perspective.
///     - mode_arg: This represents the permission of the newly created file. Directly passing to kernel.
///
/// ## Returns:
///     - return zero on success.  On error, -1 is returned and errno is set to indicate the error.
pub fn mkdir_syscall(
    cageid: u64,
    path_arg: u64,
    path_arg_cageid: u64,
    mode_arg: u64,
    mode_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_arg_cageid, cageid);
    // Note the cageid here isn't really relevant because the argument is pass-by-value.
    // But it could be checked to ensure it's not set to something unexpected.
    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "mkdir_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::mkdir(path.as_ptr(), mode) };
    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "mkdir");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/poll.2.html
///
/// Linux `poll()` syscall waits for one of a set of file descriptors to become ready to perform I/O.
/// Since we implement a file descriptor management subsystem (called `fdtables`), we convert virtual
/// file descriptors to kernel file descriptors, perform atomic polling across all fdkinds, then convert results back.
///
/// ## Arguments:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier.
///     - fds_arg: pointer to array of pollfd structures (user's perspective).
///     - nfds_arg: number of items in the fds array.
///     - timeout_arg: timeout in milliseconds (-1 = infinite, 0 = non-blocking).
///
/// ## Returns:
///     - positive value: number of file descriptors ready for I/O
///     - 0: timeout occurred with no file descriptors ready
///     - negative value: error occurred (errno set)
pub fn poll_syscall(
    cageid: u64,
    fds_arg: u64,
    fds_cageid: u64,
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
    // Validate unused arguments
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "poll_syscall", "Invalid Cage ID");
    }

    // Convert arguments
    let nfds = sc_convert_sysarg_to_usize(nfds_arg, nfds_cageid, cageid);
    let original_timeout = sc_convert_sysarg_to_i32(timeout_arg, timeout_cageid, cageid);

    // Basic bounds checking
    if nfds > 65536 {
        return syscall_error(Errno::EINVAL, "poll_syscall", "Too many file descriptors");
    }

    if fds_arg == 0 {
        return syscall_error(Errno::EFAULT, "poll_syscall", "pollfd array is null");
    }

    // Convert pollfd array from user space
    let fds_ptr = sc_convert_buf(fds_arg, fds_cageid, cageid) as *mut pollfd;
    if fds_ptr.is_null() {
        return syscall_error(Errno::EFAULT, "poll_syscall", "pollfd array is null");
    }

    // Create safe slice for pollfd array
    let fds_slice = unsafe { std::slice::from_raw_parts_mut(fds_ptr, nfds) };

    // Build index maps for O(1) lookups - avoid O(N²) performance
    let mut vfd_to_index: HashMap<i32, usize> = HashMap::new();
    let mut vfd_to_events: HashMap<i32, i16> = HashMap::new();
    
    // Clear all revents initially and build lookup maps
    for i in 0..nfds {
        fds_slice[i].revents = 0;
        
        // Build index mapping for O(1) result updates later
        vfd_to_index.insert(fds_slice[i].fd, i);
        vfd_to_events.insert(fds_slice[i].fd, fds_slice[i].events);
    }

    // Extract virtual fds from pollfd array and handle invalid fds immediately
    let mut virtual_fds = HashSet::new();
    let mut invalid_fds = Vec::new();
    
    for i in 0..nfds {
        if fds_slice[i].fd >= 0 {
            let vfd = fds_slice[i].fd as u64;
            // Check if this virtual fd exists in fdtables
            match fdtables::translate_virtual_fd(cageid, vfd) {
                Ok(_) => {
                    virtual_fds.insert(vfd);
                }
                Err(_) => {
                    // Invalid fd - mark for POLLNVAL
                    invalid_fds.push(i);
                }
            }
        }
    }

    // Handle invalid fds immediately
    let mut total_ready = 0i32;
    for &index in &invalid_fds {
        fds_slice[index].revents = libc::POLLNVAL as i16;
        total_ready += 1;
    }

    // If no valid fds to process, return immediately
    if virtual_fds.is_empty() {
        return total_ready;
    }

    // Convert virtual fds to kernel fds by fdkind using fdtables API
    let (poll_data_by_fdkind, mapping_table) = fdtables::convert_virtualfds_for_poll(cageid, virtual_fds);

    // Separate kernel-backed FDs from virtual FDs for atomic handling
    let mut all_kernel_pollfds: Vec<pollfd> = Vec::new();
    let mut kernel_to_vfd_mapping: HashMap<usize, u64> = HashMap::new();
    let mut virtual_fd_handlers: HashMap<u32, HashSet<(u64, fdtables::FDTableEntry)>> = HashMap::new();

    for (fdkind, fd_set) in poll_data_by_fdkind {
        if fdkind == FDKIND_KERNEL {
            // Collect all kernel FDs for atomic polling
            for (vfd, fdentry) in fd_set {
                // Use O(1) lookup to find original events for this virtual fd
                let events = *vfd_to_events.get(&(vfd as i32)).unwrap_or(&0);

                let kernel_index = all_kernel_pollfds.len();
                kernel_to_vfd_mapping.insert(kernel_index, vfd);
                
                all_kernel_pollfds.push(pollfd {
                    fd: fdentry.underfd as i32,
                    events,
                    revents: 0,
                });
            }
        } else {
            // Store virtual FDs for separate handling
            virtual_fd_handlers.insert(fdkind, fd_set);
        }
    }

    // Track time for consistent timeout behavior across operations
    let start_time = if original_timeout > 0 { Some(Instant::now()) } else { None };

    // Atomic poll operation for all kernel-backed FDs
    if !all_kernel_pollfds.is_empty() {
        let ret = unsafe {
            libc::poll(
                all_kernel_pollfds.as_mut_ptr(),
                all_kernel_pollfds.len() as libc::nfds_t,
                original_timeout,
            )
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "poll_syscall");
        }

        // Convert kernel results back to virtual fds
        for (kernel_index, kernel_pollfd) in all_kernel_pollfds.iter().enumerate() {
            if kernel_pollfd.revents != 0 {
                if let Some(&virtual_fd) = kernel_to_vfd_mapping.get(&kernel_index) {
                    // Use O(1) lookup to update original user array
                    if let Some(&array_index) = vfd_to_index.get(&(virtual_fd as i32)) {
                        fds_slice[array_index].revents = kernel_pollfd.revents;
                        total_ready += 1;
                    }
                }
            }
        }
    }

    // Handle virtual FDs (non-kernel) with remaining timeout
    for (fdkind, fd_set) in virtual_fd_handlers {
        // Calculate remaining timeout
        let remaining_timeout = if let Some(start) = start_time {
            let elapsed_ms = start.elapsed().as_millis() as i32;
            if original_timeout < 0 {
                original_timeout // Infinite timeout remains infinite
            } else {
                std::cmp::max(0, original_timeout - elapsed_ms)
            }
        } else {
            original_timeout
        };

        // For now, we only handle FDKIND_KERNEL. Other fdkinds would need
        // specialized implementations based on their type (e.g., in-memory pipes, 
        // virtual sockets, etc.). This is where future extensions would go.
        match fdkind {
            // Future: Handle other fdkinds here
            // FDKIND_PIPE => handle_virtual_pipe_poll(fd_set, remaining_timeout),
            // FDKIND_SOCKET => handle_virtual_socket_poll(fd_set, remaining_timeout),
            _ => {
                // For unhandled fdkinds, mark as ready for now to avoid blocking indefinitely
                // This preserves existing behavior while being explicit about the limitation
                for (vfd, _fdentry) in fd_set {
                    // Use O(1) lookup to update result
                    if let Some(&array_index) = vfd_to_index.get(&(vfd as i32)) {
                        // Set POLLERR to indicate this fdkind is not yet implemented
                        fds_slice[array_index].revents = libc::POLLERR as i16;
                        total_ready += 1;
                    }
                }
            }
        }
    }

    total_ready
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/select.2.html
///
/// Linux `select()` syscall monitors multiple file descriptors, waiting until one or more become ready 
/// for I/O operation. Since we implement a file descriptor management subsystem (called `fdtables`), we 
/// convert virtual file descriptors to kernel file descriptors, perform atomic operations across all fdkinds, 
/// then convert results back.
///
/// ## Arguments:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier.
///     - nfds_arg: highest-numbered file descriptor in any of the three sets, plus 1.
///     - readfds_arg: pointer to fd_set for monitoring read readiness (user's perspective).
///     - writefds_arg: pointer to fd_set for monitoring write readiness (user's perspective).
///     - exceptfds_arg: pointer to fd_set for monitoring exceptional conditions (user's perspective).
///     - timeout_arg: pointer to timeval structure specifying timeout (user's perspective).
///
/// ## Returns:
///     - positive value: number of file descriptors ready for I/O
///     - 0: timeout occurred with no file descriptors ready  
///     - negative value: error occurred (errno set)
pub fn select_syscall(
    cageid: u64,
    nfds_arg: u64,
    nfds_cageid: u64,
    readfds_arg: u64,
    readfds_cageid: u64,
    writefds_arg: u64,
    writefds_cageid: u64,
    exceptfds_arg: u64,
    exceptfds_cageid: u64,
    timeout_arg: u64,
    timeout_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Validate unused arguments
    if !sc_unusedarg(arg6, arg6_cageid) {
        return syscall_error(Errno::EFAULT, "select_syscall", "Invalid Cage ID");
    }

    // Convert arguments
    let nfds = sc_convert_sysarg_to_i32(nfds_arg, nfds_cageid, cageid);

    // Basic bounds checking
    if nfds < 0 || nfds > 1024 {
        return syscall_error(Errno::EINVAL, "select_syscall", "Invalid nfds value");
    }

    // Convert fd_set pointers (they can be null)
    let readfds_ptr = if readfds_arg == 0 { 
        std::ptr::null_mut() 
    } else { 
        sc_convert_buf(readfds_arg, readfds_cageid, cageid) as *mut fd_set 
    };
    
    let writefds_ptr = if writefds_arg == 0 { 
        std::ptr::null_mut() 
    } else { 
        sc_convert_buf(writefds_arg, writefds_cageid, cageid) as *mut fd_set 
    };
    
    let exceptfds_ptr = if exceptfds_arg == 0 { 
        std::ptr::null_mut() 
    } else { 
        sc_convert_buf(exceptfds_arg, exceptfds_cageid, cageid) as *mut fd_set 
    };

    let timeout_ptr = if timeout_arg == 0 { 
        std::ptr::null_mut() 
    } else { 
        sc_convert_buf(timeout_arg, timeout_cageid, cageid) as *mut timeval 
    };

    // Convert fd_set pointers to Options for fdtables API
    let readfds_opt = if readfds_ptr.is_null() { 
        None 
    } else { 
        Some(unsafe { *readfds_ptr }) 
    };
    
    let writefds_opt = if writefds_ptr.is_null() { 
        None 
    } else { 
        Some(unsafe { *writefds_ptr }) 
    };
    
    let exceptfds_opt = if exceptfds_ptr.is_null() { 
        None 
    } else { 
        Some(unsafe { *exceptfds_ptr }) 
    };

    // Store original timeout for time tracking
    let original_timeout = if timeout_ptr.is_null() {
        None
    } else {
        Some(unsafe { *timeout_ptr })
    };

    // Define which fdkinds to handle with kernel select (focusing on FDKIND_KERNEL)
    let handled_fdkinds = HashSet::from([FDKIND_KERNEL]);

    // Prepare bitmasks for select using fdtables API
    let (bitmask_tables, unhandled_tables, mapping_table) = match fdtables::prepare_bitmasks_for_select(
        cageid, 
        nfds as u64, 
        readfds_opt, 
        writefds_opt, 
        exceptfds_opt, 
        &handled_fdkinds
    ) {
        Ok(result) => result,
        Err(e) => {
            // Handle invalid file descriptors by clearing all sets and returning error
            if !readfds_ptr.is_null() { 
                unsafe { libc::FD_ZERO(&mut *readfds_ptr); } 
            }
            if !writefds_ptr.is_null() { 
                unsafe { libc::FD_ZERO(&mut *writefds_ptr); } 
            }
            if !exceptfds_ptr.is_null() { 
                unsafe { libc::FD_ZERO(&mut *exceptfds_ptr); } 
            }
            return syscall_error(Errno::EBADF, "select_syscall", "Invalid file descriptor");
        }
    };

    // Track time for consistent timeout behavior across operations
    let start_time = if original_timeout.is_some() { Some(Instant::now()) } else { None };

    let mut total_ready = 0i32;
    let mut result_read_fds: Option<fd_set> = None;
    let mut result_write_fds: Option<fd_set> = None;
    let mut result_except_fds: Option<fd_set> = None;

    // Collect all kernel operations for atomic execution
    let mut kernel_ops: Vec<(u32, i32, fd_set, Option<fd_set>, Option<fd_set>)> = Vec::new();
    let mut virtual_fd_handlers: HashMap<u32, (Option<fd_set>, Option<fd_set>, Option<fd_set>)> = HashMap::new();

    for (fdkind, (nfds_kernel, kernel_readfds)) in bitmask_tables[0].iter() {
        let kernel_writefds = bitmask_tables[1].get(fdkind).map(|(_, fds)| *fds);
        let kernel_exceptfds = bitmask_tables[2].get(fdkind).map(|(_, fds)| *fds);
        
        if *fdkind == FDKIND_KERNEL {
            // Collect kernel operations for atomic execution
            kernel_ops.push((*fdkind, *nfds_kernel as i32, *kernel_readfds, kernel_writefds, kernel_exceptfds));
        } else {
            // Store virtual FD operations for separate handling
            virtual_fd_handlers.insert(*fdkind, (Some(*kernel_readfds), kernel_writefds, kernel_exceptfds));
        }
    }

    // Execute atomic kernel select operations
    for (fdkind, nfds_kernel, mut kernel_readfds, kernel_writefds, kernel_exceptfds) in kernel_ops {
        // Calculate remaining timeout
        let current_timeout_ptr = if let (Some(start), Some(orig_timeout)) = (start_time, original_timeout) {
            let elapsed = start.elapsed();
            let elapsed_secs = elapsed.as_secs() as i64;
            let elapsed_usecs = elapsed.subsec_micros() as i64;
            
            let remaining_secs = orig_timeout.tv_sec - elapsed_secs;
            let remaining_usecs = orig_timeout.tv_usec - elapsed_usecs;
            
            if remaining_secs < 0 || (remaining_secs == 0 && remaining_usecs <= 0) {
                // Timeout already elapsed, use zero timeout
                let mut zero_timeout = timeval { tv_sec: 0, tv_usec: 0 };
                &mut zero_timeout as *mut timeval
            } else {
                // Adjust for any negative microseconds
                let (adj_secs, adj_usecs) = if remaining_usecs < 0 {
                    (remaining_secs - 1, remaining_usecs + 1_000_000)
                } else {
                    (remaining_secs, remaining_usecs)
                };
                
                let mut adjusted_timeout = timeval { tv_sec: adj_secs, tv_usec: adj_usecs };
                &mut adjusted_timeout as *mut timeval
            }
        } else {
            timeout_ptr
        };

        // Call kernel select for this fdkind
        let ret = unsafe {
            libc::select(
                nfds_kernel,
                &mut kernel_readfds as *mut fd_set,
                kernel_writefds.as_ref().map_or(std::ptr::null_mut(), |fds| fds as *const fd_set as *mut fd_set),
                kernel_exceptfds.as_ref().map_or(std::ptr::null_mut(), |fds| fds as *const fd_set as *mut fd_set),
                current_timeout_ptr,
            )
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "select_syscall");
        }

        if ret > 0 {
            // Convert results back to virtual fd_sets and accumulate results
            
            // Handle read fds
            if !readfds_ptr.is_null() {
                let (read_count, virt_readfds) = fdtables::get_one_virtual_bitmask_from_select_result(
                    fdkind, 
                    nfds as u64, 
                    Some(kernel_readfds), 
                    unhandled_tables[0].get(&fdkind).cloned().unwrap_or_default(), 
                    result_read_fds.or(readfds_opt), 
                    &mapping_table
                );
                
                if read_count > 0 {
                    result_read_fds = virt_readfds;
                    total_ready += read_count as i32;
                }
            }

            // Handle write fds  
            if !writefds_ptr.is_null() && kernel_writefds.is_some() {
                let (write_count, virt_writefds) = fdtables::get_one_virtual_bitmask_from_select_result(
                    fdkind, 
                    nfds as u64, 
                    kernel_writefds, 
                    unhandled_tables[1].get(&fdkind).cloned().unwrap_or_default(), 
                    result_write_fds.or(writefds_opt), 
                    &mapping_table
                );
                
                if write_count > 0 {
                    result_write_fds = virt_writefds;
                    total_ready += write_count as i32;
                }
            }

            // Handle except fds
            if !exceptfds_ptr.is_null() && kernel_exceptfds.is_some() {
                let (except_count, virt_exceptfds) = fdtables::get_one_virtual_bitmask_from_select_result(
                    fdkind, 
                    nfds as u64, 
                    kernel_exceptfds, 
                    unhandled_tables[2].get(&fdkind).cloned().unwrap_or_default(), 
                    result_except_fds.or(exceptfds_opt), 
                    &mapping_table
                );
                
                if except_count > 0 {
                    result_except_fds = virt_exceptfds;
                    total_ready += except_count as i32;
                }
            }
        }
    }

    // Handle virtual FDs (non-kernel) - for future fdkind implementations
    for (fdkind, (_read_set, _write_set, _except_set)) in virtual_fd_handlers {
        // Calculate remaining timeout (similar to above)
        // For now, we only handle FDKIND_KERNEL. Other fdkinds would need
        // specialized implementations here.
        match fdkind {
            // Future: Handle other fdkinds here
            // FDKIND_PIPE => handle_virtual_pipe_select(...),
            // FDKIND_SOCKET => handle_virtual_socket_select(...),
            _ => {
                // For unhandled fdkinds, we don't mark them as ready
                // This avoids false positives but may cause blocking if only
                // virtual FDs are being monitored
            }
        }
    }

    // Update user's fd_sets with the final results
    if !readfds_ptr.is_null() {
        unsafe { 
            *readfds_ptr = result_read_fds.unwrap_or_else(|| {
                let mut empty_set = std::mem::zeroed();
                libc::FD_ZERO(&mut empty_set);
                empty_set
            });
        }
    }
    
    if !writefds_ptr.is_null() {
        unsafe { 
            *writefds_ptr = result_write_fds.unwrap_or_else(|| {
                let mut empty_set = std::mem::zeroed();
                libc::FD_ZERO(&mut empty_set);
                empty_set
            });
        }
    }
    
    if !exceptfds_ptr.is_null() {
        unsafe { 
            *exceptfds_ptr = result_except_fds.unwrap_or_else(|| {
                let mut empty_set = std::mem::zeroed();
                libc::FD_ZERO(&mut empty_set);
                empty_set
            });
        }
    }

    total_ready
}