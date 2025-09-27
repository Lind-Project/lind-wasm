use cage::signal_check_trigger;
use fdtables;
use fdtables::epoll_event;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::lind_platform_const::FDKIND_KERNEL;
use sysdefs::constants::net_const::{EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD};
use sysdefs::data::fs_struct::EpollEvent;
use typemap::datatype_conversion::*;
use typemap::network_helpers::{convert_host_sockaddr, convert_sockpair, copy_out_sockaddr};
use typemap::cage_helpers::convert_fd_to_host;
use typemap::path_conversion::{sc_convert_path_to_host, sc_convert_uaddr_to_host};
use sysdefs::data::net_struct::SockAddr;
use libc::*;
use std::ptr;


/// Reference to Linux: https://man7.org/linux/man-pages/man2/poll.2.html
///
/// Linux `poll()` syscall waits for one of a set of file descriptors to become ready to perform I/O.
///
/// ## Implementation Approach:
///
/// 1. **Early Validation**: Check `nfds` limits and null pointers before expensive operations
/// 2. **FD Classification**: Use `fdtables::convert_virtualfds_for_poll()` to separate kernel-backed FDs from invalid ones
/// 3. **Batch Processing**:
///    - Invalid FDs → mark as `POLLNVAL` immediately
///    - Kernel FDs → collect into array for single `libc::poll()` call
///    - Virtual FDs → ignored (we only handle FDs with underlying kernel FDs)
/// 4. **Result Conversion**: Convert kernel poll results back to virtual FDs using fdtables mapping
/// 5. **Update User Array**: Use O(1) lookups to update original user array with results
///
/// This maintains POSIX poll semantics while handling Lind's FD virtualization efficiently.
///
/// ## Arguments:
///     - cageid: current cage identifier.
///     - fds_arg: pointer to array of pollfd structures (user's perspective).
///     - fds_cageid: cage ID for fds_arg validation.
///     - nfds_arg: number of items in the fds array.
///     - nfds_cageid: cage ID for nfds_arg validation.
///     - timeout_arg: timeout in milliseconds (-1 = infinite, 0 = non-blocking).
///     - timeout_cageid: cage ID for timeout_arg validation.
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

    // Basic bounds checking - validate arguments before conversion - FD_PER_PROCESS_MAX is defined in fdtables constants
    if nfds_arg > fdtables::FD_PER_PROCESS_MAX {
        return syscall_error(Errno::EINVAL, "poll_syscall", "Too many file descriptors");
    }

    if nfds_arg == 0 {
        return 0; // No FDs to poll
    }

    if fds_arg == 0 {
        return syscall_error(Errno::EFAULT, "poll_syscall", "pollfd array is null");
    }

    // Convert arguments after validation
    let nfds = sc_convert_sysarg_to_usize(nfds_arg, nfds_cageid, cageid);
    let original_timeout = sc_convert_sysarg_to_i32(timeout_arg, timeout_cageid, cageid);

    // Convert pollfd array from user space
    let fds_ptr = sc_convert_buf(fds_arg, fds_cageid, cageid) as *mut libc::pollfd;
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

    // Extract virtual fds from pollfd array - let fdtables handle invalid FDs
    let mut virtual_fds = HashSet::new();

    for i in 0..nfds {
        if fds_slice[i].fd >= 0 {
            virtual_fds.insert(fds_slice[i].fd as u64);
        }
    }

    // If no FDs to process, return immediately
    if virtual_fds.is_empty() {
        return 0;
    }

    // Convert virtual fds to kernel fds by fdkind using fdtables API
    let (poll_data_by_fdkind, fdtables_mapping_table) =
        fdtables::convert_virtualfds_for_poll(cageid, virtual_fds);

    // Process kernel-backed FDs and handle invalid FDs
    let mut all_kernel_pollfds: Vec<libc::pollfd> = Vec::new();
    let mut kernel_to_vfd_mapping: HashMap<usize, u64> = HashMap::new();
    let mut total_ready = 0i32;

    for (fdkind, fd_set) in poll_data_by_fdkind {
        match fdkind {
            FDKIND_KERNEL => {
                // Collect all kernel FDs for polling
                for (vfd, fdentry) in fd_set {
                    // Use O(1) lookup to find original events for this virtual fd
                    let events = *vfd_to_events.get(&(vfd as i32)).unwrap_or(&0);

                    let kernel_index = all_kernel_pollfds.len();
                    kernel_to_vfd_mapping.insert(kernel_index, vfd);

                    all_kernel_pollfds.push(libc::pollfd {
                        fd: fdentry.underfd as i32,
                        events,
                        revents: 0,
                    });
                }
            }
            fdtables::FDT_INVALID_FD => {
                // Handle invalid FDs immediately - fdtables has already identified them
                for (vfd, _fdentry) in fd_set {
                    if let Some(&array_index) = vfd_to_index.get(&(vfd as i32)) {
                        fds_slice[array_index].revents = libc::POLLNVAL as i16;
                        total_ready += 1;
                    }
                }
            }
            _ => {
                // Handle non-kernel FDs - consistent with old implementation error handling
                return syscall_error(Errno::EBADFD, "poll_syscall", "Invalid fdkind");
            }
        }
    }

    // Poll all kernel-backed fds with timeout/signal checking loop (consistent with old implementation)
    if !all_kernel_pollfds.is_empty() {
        let start_time = Instant::now();
        let timeout_duration = if original_timeout >= 0 {
            Some(std::time::Duration::from_millis(original_timeout as u64))
        } else {
            None // Infinite timeout
        };

        let ret;
        loop {
            let poll_ret = unsafe {
                libc::poll(
                    all_kernel_pollfds.as_mut_ptr(),
                    all_kernel_pollfds.len() as libc::nfds_t,
                    original_timeout,
                )
            };

            if poll_ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "poll_syscall");
            }

            // Check for ready FDs or timeout
            if poll_ret > 0 {
                ret = poll_ret;
                break;
            }

            // Check for timeout (if specified)
            if let Some(timeout_dur) = timeout_duration {
                if start_time.elapsed() >= timeout_dur {
                    ret = 0; // Timeout occurred
                    break;
                }
            }

            // Check for signals - consistent with higher-level approach
            if signal_check_trigger(cageid) {
                return syscall_error(Errno::EINTR, "poll_syscall", "interrupted");
            }
        }

        // Convert kernel results back to virtual fds using fdtables helper
        for (kernel_index, kernel_pollfd) in all_kernel_pollfds.iter().enumerate() {
            if kernel_pollfd.revents != 0 {
                if let Some(&virtual_fd) = kernel_to_vfd_mapping.get(&kernel_index) {
                    // Use fdtables helper to convert kernel fd back to virtual fd
                    if let Some(converted_vfd) = fdtables::convert_poll_result_back_to_virtual(
                        FDKIND_KERNEL,
                        kernel_pollfd.fd as u64,
                        &fdtables_mapping_table,
                    ) {
                        // Use O(1) lookup to update original user array
                        if let Some(&array_index) = vfd_to_index.get(&(converted_vfd as i32)) {
                            fds_slice[array_index].revents = kernel_pollfd.revents;
                            total_ready += 1;
                        }
                    }
                }
            }
        }
    }

    total_ready
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/select.2.html
///
/// Linux `select()` syscall waits for one of a set of file descriptors to become ready to perform I/O.
///
/// ## Implementation Approach:
///
/// The design logic for select is first to categorize the file descriptors (fds) received from the user based on FDKIND.
/// Specifically, kernel fds are passed to the underlying libc select, while impipe and imsock fds would be processed by the
/// in-memory system. Afterward, the results are combined and consolidated accordingly.
///
/// (Note: Currently, only kernel fds are supported. The implementation for in-memory pipes is commented out and will require
/// further integration and testing once in-memory pipe support is added.)
///
/// select() will return:
///     - the total number of bits that are set in readfds, writefds, errorfds
///     - 0, if the timeout expired before any file descriptors became ready
///     - -1, fail
///
/// ## Arguments:
///     - cageid: current cage identifier.
///     - nfds_arg: highest-numbered file descriptor in any of the three sets, plus 1.
///     - nfds_cageid: cage ID for nfds_arg validation.
///     - readfds_arg: pointer to fd_set for read file descriptors (user's perspective).
///     - readfds_cageid: cage ID for readfds_arg validation.
///     - writefds_arg: pointer to fd_set for write file descriptors (user's perspective).
///     - writefds_cageid: cage ID for writefds_arg validation.
///     - exceptfds_arg: pointer to fd_set for exception file descriptors (user's perspective).
///     - exceptfds_cageid: cage ID for exceptfds_arg validation.
///     - timeout_arg: pointer to timeval structure for timeout (user's perspective).
///     - timeout_cageid: cage ID for timeout_arg validation.
///     - arg6: unused argument.
///     - arg6_cageid: cage ID for arg6 validation.
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

    // Convert fd_set pointers - they can be null
    let readfds_ptr = if readfds_arg != 0 {
        Some(sc_convert_buf(readfds_arg, readfds_cageid, cageid) as *mut libc::fd_set)
    } else {
        None
    };

    let writefds_ptr = if writefds_arg != 0 {
        Some(sc_convert_buf(writefds_arg, writefds_cageid, cageid) as *mut libc::fd_set)
    } else {
        None
    };

    let exceptfds_ptr = if exceptfds_arg != 0 {
        Some(sc_convert_buf(exceptfds_arg, exceptfds_cageid, cageid) as *mut libc::fd_set)
    } else {
        None
    };

    // Convert timeout pointer - can be null
    let timeout_ptr = if timeout_arg != 0 {
        Some(sc_convert_buf(timeout_arg, timeout_cageid, cageid) as *mut libc::timeval)
    } else {
        None
    };

    // Create fdkindset for fdtables processing
    let mut fdkindset = HashSet::new();
    fdkindset.insert(FDKIND_KERNEL);

    // Prepare bitmasks for select using fdtables
    let (selectbittables, unparsedtables, mappingtable) =
        match fdtables::prepare_bitmasks_for_select(
            cageid,
            nfds as u64,
            readfds_ptr.map(|ptr| unsafe { *ptr }),
            writefds_ptr.map(|ptr| unsafe { *ptr }),
            exceptfds_ptr.map(|ptr| unsafe { *ptr }),
            &fdkindset,
        ) {
            Ok(result) => result,
            Err(_) => {
                return syscall_error(
                    Errno::EINVAL,
                    "select_syscall",
                    "Failed to prepare bitmasks",
                )
            }
        };

    // Extract kernel fd_sets from selectbittables
    // In select, each fd_set is allowed to contain empty values, as it's possible for the user to input a mixture of pure
    // virtual_fds and those with underlying real file descriptors. This means we need to check each fd_set separately to
    // handle both types of descriptors properly. The goal here is to ensure that each fd_set (read, write, error) is correctly
    // initialized. To handle cases where selectbittables does not contain an entry at the expected index or where it doesn't
    // include a FDKIND_KERNEL entry, the code assigns a default value with an initialized fd_set and an nfd of 0.
    let (readnfd, mut real_readfds) = selectbittables
        .get(0)
        .and_then(|table| table.get(&FDKIND_KERNEL).cloned())
        .unwrap_or((0, fdtables::_init_fd_set()));
    let (writenfd, mut real_writefds) = selectbittables
        .get(1)
        .and_then(|table| table.get(&FDKIND_KERNEL).cloned())
        .unwrap_or((0, fdtables::_init_fd_set()));
    let (errornfd, mut real_errorfds) = selectbittables
        .get(2)
        .and_then(|table| table.get(&FDKIND_KERNEL).cloned())
        .unwrap_or((0, fdtables::_init_fd_set()));

    let mut realnewnfds = readnfd.max(writenfd).max(errornfd);

    // Handle timeout setup
    let start_time = Instant::now();
    let mut timeout = if let Some(timeout_ptr) = timeout_ptr {
        unsafe { *timeout_ptr }
    } else {
        libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        }
    };

    let mut ret;
    loop {
        let mut tmp_readfds = real_readfds.clone();
        let mut tmp_writefds = real_writefds.clone();
        let mut tmp_errorfds = real_errorfds.clone();

        // Call libc select with proper null handling
        // nfds should be the highest-numbered file descriptor + 1
        ret = unsafe {
            libc::select(
                (realnewnfds + 1) as i32,
                if readfds_ptr.is_some() {
                    &mut tmp_readfds as *mut _
                } else {
                    std::ptr::null_mut()
                },
                if writefds_ptr.is_some() {
                    &mut tmp_writefds as *mut _
                } else {
                    std::ptr::null_mut()
                },
                if exceptfds_ptr.is_some() {
                    &mut tmp_errorfds as *mut _
                } else {
                    std::ptr::null_mut()
                },
                if timeout_ptr.is_some() {
                    &mut timeout as *mut _
                } else {
                    std::ptr::null_mut()
                },
            )
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "select_syscall");
        }

        // Check for timeout or successful result
        if ret > 0
            || (timeout_ptr.is_some()
                && start_time.elapsed().as_millis()
                    > (timeout.tv_sec as u128 * 1000 + timeout.tv_usec as u128 / 1000))
        {
            real_readfds = tmp_readfds;
            real_writefds = tmp_writefds;
            real_errorfds = tmp_errorfds;
            break;
        }

        // Check for signals
        if signal_check_trigger(cageid) {
            return syscall_error(Errno::EINTR, "select_syscall", "interrupted");
        }
    }

    let mut unreal_read = HashSet::new();
    let mut unreal_write = HashSet::new();

    // Revert result using fdtables helper
    let (read_flags, read_result) = fdtables::get_one_virtual_bitmask_from_select_result(
        FDKIND_KERNEL,
        realnewnfds as u64,
        Some(real_readfds),
        unreal_read,
        None,
        &mappingtable,
    );

    if let Some(readfds_ptr) = readfds_ptr {
        if let Some(read_result) = read_result {
            unsafe { *readfds_ptr = read_result };
        }
    }

    let (write_flags, write_result) = fdtables::get_one_virtual_bitmask_from_select_result(
        FDKIND_KERNEL,
        realnewnfds as u64,
        Some(real_writefds),
        unreal_write,
        None,
        &mappingtable,
    );

    if let Some(writefds_ptr) = writefds_ptr {
        if let Some(write_result) = write_result {
            unsafe { *writefds_ptr = write_result };
        }
    }

    let (error_flags, error_result) = fdtables::get_one_virtual_bitmask_from_select_result(
        FDKIND_KERNEL,
        realnewnfds as u64,
        Some(real_errorfds),
        HashSet::new(), // Assuming there are no unreal errorsets
        None,
        &mappingtable,
    );

    if let Some(exceptfds_ptr) = exceptfds_ptr {
        if let Some(error_result) = error_result {
            unsafe { *exceptfds_ptr = error_result };
        }
    }

    // The total number of descriptors ready
    (read_flags + write_flags + error_flags) as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_create.2.html
///
/// Linux `epoll_create()` creates an epoll instance and returns a file descriptor referring to that instance.
///
/// ## Implementation Approach:
///
/// Uses the fdtables infrastructure to create a virtual epoll file descriptor that maps to an internal
/// epoll instance. The size parameter is ignored (as per Linux behavior) and the epoll instance is
/// created using fdtables::epoll_create_empty().
///
/// ## Arguments:
///     - cageid: current cage identifier.
///     - size_arg: hint for the size of the epoll instance (ignored in current implementation).
///     - size_cageid: cage ID for size_arg validation.
///     - arg2-arg6: unused arguments with their respective cage IDs.
///
/// ## Returns:
///     - positive value: file descriptor for the new epoll instance
///     - negative value: error occurred (errno set)
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
    // Validate unused arguments
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "epoll_create_syscall", "Invalid Cage ID");
    }

    // Convert size argument
    let size = sc_convert_sysarg_to_i32(size_arg, size_cageid, cageid);

    // Create the kernel epoll instance
    let kernel_fd = unsafe { libc::epoll_create(size) };

    if kernel_fd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "epoll_create_syscall");
    }

    // Get the virtual epfd
    let virtual_epfd =
        fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, false, 0).unwrap();

    // Return virtual epfd
    virtual_epfd as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_ctl.2.html
///
/// Linux `epoll_ctl()` performs control operations on an epoll instance.
///
/// ## Implementation Approach:
///
/// Uses the fdtables infrastructure to manage virtual epoll file descriptors and their associated
/// file descriptors. The function translates virtual FDs and validates operations before calling
/// the fdtables::virtualize_epoll_ctl() function.
///
/// ## Arguments:
///     - cageid: current cage identifier.
///     - epfd_arg: epoll file descriptor.
///     - epfd_cageid: cage ID for epfd_arg validation.
///     - op_arg: control operation (EPOLL_CTL_ADD, EPOLL_CTL_MOD, EPOLL_CTL_DEL).
///     - op_cageid: cage ID for op_arg validation.
///     - fd_arg: target file descriptor.
///     - fd_cageid: cage ID for fd_arg validation.
///     - event_arg: pointer to epoll_event structure.
///     - event_cageid: cage ID for event_arg validation.
///     - arg5-arg6: unused arguments with their respective cage IDs.
///
/// ## Returns:
///     - 0: operation completed successfully
///     - negative value: error occurred (errno set)
pub fn epoll_ctl_syscall(
    cageid: u64,
    epfd_arg: u64,
    epfd_cageid: u64,
    op_arg: u64,
    op_cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    event_arg: u64,
    event_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Validate unused arguments
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "epoll_ctl_syscall", "Invalid Cage ID");
    }

    // Convert arguments
    let epfd = sc_convert_sysarg_to_i32(epfd_arg, epfd_cageid, cageid);
    let op = sc_convert_sysarg_to_i32(op_arg, op_cageid, cageid);
    let fd = sc_convert_sysarg_to_i32(fd_arg, fd_cageid, cageid);
    let virtfd = fd as u64;

    // Validate operation
    if op != EPOLL_CTL_ADD && op != EPOLL_CTL_MOD && op != EPOLL_CTL_DEL {
        return syscall_error(Errno::EINVAL, "epoll_ctl_syscall", "Invalid operation");
    }

    // Translate virtual FDs to kernel FDs
    let wrappedepfd = fdtables::translate_virtual_fd(cageid, epfd as u64);
    let wrappedvfd = fdtables::translate_virtual_fd(cageid, virtfd);
    if wrappedvfd.is_err() || wrappedepfd.is_err() {
        return syscall_error(Errno::EBADF, "epoll_ctl_syscall", "Bad File Descriptor");
    }
    let vepfd = wrappedepfd.unwrap();
    let vfd = wrappedvfd.unwrap();

    // Convert epoll_event from user space
    let event_ptr = if event_arg != 0 {
        Some(sc_convert_buf(event_arg, event_cageid, cageid) as *mut EpollEvent)
    } else {
        None
    };

    // For EPOLL_CTL_DEL, event can be null
    if event_ptr.is_none() && op != EPOLL_CTL_DEL {
        return syscall_error(
            Errno::EFAULT,
            "epoll_ctl_syscall",
            "event pointer is null for non-DEL operation",
        );
    }

    // Get user event data for both kernel and virtual operations
    let user_event = if let Some(event_ptr) = event_ptr {
        if event_ptr.is_null() {
            return syscall_error(Errno::EFAULT, "epoll_ctl_syscall", "event pointer is null");
        }
        unsafe { *event_ptr }
    } else {
        // For EPOLL_CTL_DEL, create a dummy event
        EpollEvent { events: 0, fd: 0 }
    };

    // Create kernel epoll_event with kernel FD in u64 field
    let mut kernel_epoll_event = libc::epoll_event {
        events: user_event.events,
        u64: vfd.underfd, // Use kernel FD for kernel call
    };

    // Call actual kernel epoll_ctl
    let ret = unsafe {
        libc::epoll_ctl(
            vepfd.underfd as i32,
            op,
            vfd.underfd as i32,
            &mut kernel_epoll_event,
        )
    };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "epoll_ctl_syscall");
    }

    // After successful kernel operation, update fdtables virtual mapping
    let fdtables_event = fdtables::epoll_event {
        events: kernel_epoll_event.events,
        u64: kernel_epoll_event.u64,
    };
    match fdtables::virtualize_epoll_ctl(cageid, epfd as u64, op, virtfd, fdtables_event) {
        Ok(()) => 0,
        Err(err) => {
            // If fdtables operation fails after successful kernel operation,
            // we should ideally roll back the kernel operation, but for now
            // we'll just return the error
            handle_errno(err as i32, "epoll_ctl_syscall")
        }
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_wait.2.html
///
/// Linux `epoll_wait()` waits for events on an epoll file descriptor.
///
/// ## Implementation Approach:
///
/// Uses the fdtables infrastructure to get virtual epoll data and handles both kernel-backed
/// and in-memory file descriptors. For kernel FDs, calls libc::epoll_wait() on the underlying
/// kernel epoll FD. For in-memory FDs, implements custom polling logic. Results are converted
/// back to virtual FDs and written to the user-space events array.
///
/// ## Arguments:
///     - cageid: current cage identifier.
///     - epfd_arg: epoll file descriptor.
///     - epfd_cageid: cage ID for epfd_arg validation.
///     - events_arg: pointer to array of epoll_event structures.
///     - events_cageid: cage ID for events_arg validation.
///     - maxevents_arg: maximum number of events to return.
///     - maxevents_cageid: cage ID for maxevents_arg validation.
///     - timeout_arg: timeout in milliseconds (-1 = infinite, 0 = non-blocking).
///     - timeout_cageid: cage ID for timeout_arg validation.
///     - arg5-arg6: unused arguments with their respective cage IDs.
///
/// ## Returns:
///     - positive value: number of file descriptors ready for I/O
///     - 0: timeout occurred with no file descriptors ready
///     - negative value: error occurred (errno set)
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
    // Validate unused arguments
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "epoll_wait_syscall", "Invalid Cage ID");
    }

    // Convert arguments
    let epfd = sc_convert_sysarg_to_i32(epfd_arg, epfd_cageid, cageid);
    let maxevents = sc_convert_sysarg_to_i32(maxevents_arg, maxevents_cageid, cageid);
    let timeout = sc_convert_sysarg_to_i32(timeout_arg, timeout_cageid, cageid);

    // Validate maxevents
    if maxevents <= 0 {
        return syscall_error(
            Errno::EINVAL,
            "epoll_wait_syscall",
            "maxevents must be positive",
        );
    }

    if events_arg == 0 {
        return syscall_error(Errno::EFAULT, "epoll_wait_syscall", "events array is null");
    }

    // Convert events array from user space
    let events_ptr = sc_convert_buf(events_arg, events_cageid, cageid) as *mut EpollEvent;
    if events_ptr.is_null() {
        return syscall_error(Errno::EFAULT, "epoll_wait_syscall", "events array is null");
    }

    // Create safe slice for events array
    let events_slice = unsafe { std::slice::from_raw_parts_mut(events_ptr, maxevents as usize) };

    // Get virtual epoll wait data from fdtables
    let epoll_data = match fdtables::get_virtual_epoll_wait_data(cageid, epfd as u64) {
        Ok(data) => data,
        Err(err) => return handle_errno(err as i32, "epoll_wait_syscall"),
    };

    // Check if epoll instance is empty
    if epoll_data.is_empty() {
        return 0;
    }

    // Handle timeout setup (following select_syscall pattern)
    let start_time = Instant::now();
    let timeout_duration = if timeout == -1 {
        // Infinite timeout - use a very large duration
        std::time::Duration::from_millis(u64::MAX)
    } else if timeout == 0 {
        // Non-blocking
        std::time::Duration::from_millis(0)
    } else {
        std::time::Duration::from_millis(timeout as u64)
    };

    let mut total_ready = 0i32;

    // Process each fdkind in the epoll data
    for (fdkind, fd_events_map) in epoll_data {
        if fdkind == FDKIND_KERNEL {
            // Handle kernel-backed FDs
            let mut kernel_events: Vec<libc::epoll_event> = Vec::with_capacity(maxevents as usize);

            // Get the underlying kernel epoll FD for this fdkind
            let kernel_epfd = match fdtables::epoll_get_underfd_hashmap(cageid, epfd as u64) {
                Ok(underfd_map) => {
                    match underfd_map.get(&fdkind) {
                        Some(epfd) => *epfd as i32,
                        None => continue, // No kernel epoll FD for this fdkind
                    }
                }
                Err(_) => continue, // Skip if we can't get the mapping
            };

            // Initialize kernel events array
            for _ in 0..maxevents {
                kernel_events.push(libc::epoll_event { events: 0, u64: 0 });
            }

            let mut ret;
            loop {
                ret = unsafe {
                    libc::epoll_wait(
                        kernel_epfd,
                        kernel_events.as_mut_ptr(),
                        maxevents,
                        if timeout == -1 { -1 } else { timeout },
                    )
                };

                if ret < 0 {
                    let errno = get_errno();
                    return handle_errno(errno, "epoll_wait_syscall");
                }

                // Check for timeout or successful result
                if ret > 0 || (timeout != -1 && start_time.elapsed() > timeout_duration) {
                    break;
                }

                // Check for signals
                if signal_check_trigger(cageid) {
                    return syscall_error(Errno::EINTR, "epoll_wait_syscall", "interrupted");
                }
            }

            // Convert kernel results back to virtual FDs
            for i in 0..ret as usize {
                if total_ready >= maxevents {
                    break;
                }

                let kernel_event = &kernel_events[i];

                // Find the virtual FD that corresponds to this kernel FD
                for (virtfd, user_event) in &fd_events_map {
                    if let Some(virtfd_entry) = fdtables::translate_virtual_fd(cageid, *virtfd).ok()
                    {
                        if virtfd_entry.underfd == kernel_event.u64 {
                            // Found the matching virtual FD, update user array
                            events_slice[total_ready as usize] = EpollEvent {
                                events: kernel_event.events,
                                fd: *virtfd as i32,
                            };
                            total_ready += 1;
                            break;
                        }
                    }
                }
            }
        } else {
            // Handle in-memory FDs (custom polling logic)
            // For now, we'll implement a simple approach similar to the old implementation
            // This would need to be expanded based on the specific in-memory FD types

            // Check for timeout on non-blocking call
            if timeout == 0 {
                continue; // Non-blocking, no in-memory FDs ready
            }

            // For in-memory FDs, we would implement custom polling logic here
            // This is a placeholder for future implementation
            for (virtfd, _user_event) in &fd_events_map {
                if total_ready >= maxevents {
                    break;
                }

                // Placeholder: check if in-memory FD is ready
                // This would need actual implementation based on the FD type
                // For now, we'll skip in-memory FDs
                continue;
            }
        }
    }

    total_ready
}


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
    let cloexec = (socktype & libc::SOCK_CLOEXEC) != 0;

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
    let addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);

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
    let addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);

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
    let addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);

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
    let optval = sc_convert_to_u8_mut(optval_arg, optval_cageid, cageid);
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/send.2.html
///
/// The Linux `send()` syscall is used to transmit a message through a socket.
/// This implementation extracts the virtual file descriptor and buffer from the current cage,
/// then forwards the request to the host kernel with the provided flags.
///
/// ## Input:
///     - cageid: current cageid
///     - fd_arg: virtual file descriptor indicating the socket to send data on
///     - buf_arg: pointer to the message buffer in user memory
///     - buflen_arg: length of the message to be sent
///     - flags_arg: bitmask of flags influencing message transmission behavior
///
/// ## Return:
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
    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "send_syscall", "Invalid Cage ID");
    }

    let ret = unsafe { libc::send(fd as i32, buf as *const c_void, buflen, flags) as i32};

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "send");
    }

    ret
}

<<<<<<< HEAD
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
    let buf = sc_convert_to_u8_mut(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flag = sc_convert_sysarg_to_i32(flag_arg, flag_cageid, cageid);
    let sockaddr = sc_convert_to_u8_mut(sockaddr_arg, sockaddr_cageid, cageid);
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
    let buf = sc_convert_to_u8_mut(buf_arg, buf_cageid, cageid);
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
=======
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
>>>>>>> add-netcalls-3i
///
/// ## Return:
///     - On success: 0  
///     - On failure: negative errno indicating the error
<<<<<<< HEAD
pub fn gethostname_syscall(
    cageid: u64,
    name_arg: u64,
    name_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
=======
pub fn shutdown_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    how_arg: u64,
    how_cageid: u64,
>>>>>>> add-netcalls-3i
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
<<<<<<< HEAD
    let name = sc_convert_to_u8_mut(name_arg, name_cageid, cageid);
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);

=======
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let how = sc_convert_sysarg_to_i32(how_arg, how_cageid, cageid);

    // would check when `secure` flag has been set during compilation, 
    // no-op by default
>>>>>>> add-netcalls-3i
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
<<<<<<< HEAD
        return syscall_error(Errno::EFAULT, "gethostname_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::gethostname(name as *mut i8, len) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "gethostname");
=======
        return syscall_error(Errno::EFAULT, "shutdown_syscall", "Invalid Cage ID");
    }

    let ret = unsafe { libc::shutdown(fd, how) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "shutdown");
>>>>>>> add-netcalls-3i
    }

    ret
}

<<<<<<< HEAD
/// Reference to Linux: https://man7.org/linux/man-pages/man2/getsockopt.2.html
///
/// The Linux `getsockopt()` syscall retrieves the value of a socket option.
/// This implementation retrieves the virtual file descriptor, option level, and option name
/// from the current cage, and writes the result to the provided user-space buffer.
=======
/// Reference to Linux: https://man7.org/linux/man-pages/man2/getsockname.2.html
///
/// The Linux `getsockname()` syscall retrieves the current address to which the socket
/// is bound.  
/// This implementation resolves the virtual file descriptor to the host kernel file descriptor,
/// converts the user-space sockaddr structure into its host representation, and invokes
/// the host kernel `getsockname()`.
>>>>>>> add-netcalls-3i
///
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the socket
<<<<<<< HEAD
///     - level_arg: protocol level at which the option resides
///     - optname_arg: name of the option to retrieve
///     - optval_arg: pointer to a buffer to store the option value
=======
///     - addr_arg: pointer to a buffer in user space where the address will be stored
>>>>>>> add-netcalls-3i
///
/// ## Return:
///     - On success: 0  
///     - On failure: negative errno indicating the error
<<<<<<< HEAD
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
    let optval = sc_convert_to_u8_mut(optval_arg, optval_cageid, cageid);

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
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the connected socket
///     - addr_arg: pointer to a buffer in user space to store the peer address
///
/// ## Return:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub fn getpeername_syscall(
=======
pub fn getsockname_syscall(
>>>>>>> add-netcalls-3i
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
<<<<<<< HEAD
    let addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);

=======
    let addr = sc_convert_addr_to_host(addr_arg, addr_cageid, cageid);
    
    // would check when `secure` flag has been set during compilation, 
    // no-op by default
>>>>>>> add-netcalls-3i
    if !(sc_unusedarg(arg3, arg3_cageid)
    &&sc_unusedarg(arg4, arg4_cageid)
    && sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
<<<<<<< HEAD
        return syscall_error(Errno::EFAULT, "getpeername_syscall", "Invalide Cage ID");
    }

    let (finalsockaddr, mut addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);
    let ret = unsafe { libc::getpeername(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "getpeername");
=======
        return syscall_error(Errno::EFAULT, "getsockname_syscall", "Invalid Cage ID");
    }
    
    let (finalsockaddr, addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);

    let ret = unsafe { libc::getsockname(fd as i32, finalsockaddr, addrlen as *mut u32) };

    if ret < 0  {
        let errno = get_errno();
        return handle_errno(errno, "getsockname");
>>>>>>> add-netcalls-3i
    }

    ret
}
<<<<<<< HEAD

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
    
    if !(sc_unusedarg(arg5, arg5_cageid)
    && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "socketpair_syscall", "Invalide Cage ID");
    }

    let mut kernel_socket_vector: [i32; 2] = [0, 0];

    let ret = unsafe { libc::socketpair(domain, typ, protocol, kernel_socket_vector.as_mut_ptr()) };
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
=======
>>>>>>> add-netcalls-3i
