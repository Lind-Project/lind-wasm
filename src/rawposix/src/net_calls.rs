use typemap::datatype_conversion::*;
use sysdefs::constants::err_const::{syscall_error, Errno, get_errno, handle_errno};
use sysdefs::constants::lind_platform_const::FDKIND_KERNEL;
use cage::{signal_check_trigger};
use fdtables;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

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
    let (poll_data_by_fdkind, fdtables_mapping_table) = fdtables::convert_virtualfds_for_poll(cageid, virtual_fds);

    // Process kernel-backed FDs and handle invalid FDs
    let mut all_kernel_pollfds: Vec<libc::pollfd> = Vec::new();
    let mut kernel_to_vfd_mapping: HashMap<usize, u64> = HashMap::new();
    let mut total_ready = 0i32;

    for (fdkind, fd_set) in poll_data_by_fdkind {
        if fdkind == FDKIND_KERNEL {
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
        } else if fdkind == fdtables::FDT_INVALID_FD {
            // Handle invalid FDs immediately - fdtables has already identified them
            for (vfd, _fdentry) in fd_set {
                if let Some(&array_index) = vfd_to_index.get(&(vfd as i32)) {
                    fds_slice[array_index].revents = libc::POLLNVAL as i16;
                    total_ready += 1;
                }
            }
        }
        // Ignore other fdkind types - we only handle FDs with underlying kernel FDs
    }

    // Poll all kernel-backed fds with a single kernel libc call
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

        // Convert kernel results back to virtual fds using fdtables helper
        for (kernel_index, kernel_pollfd) in all_kernel_pollfds.iter().enumerate() {
            if kernel_pollfd.revents != 0 {
                if let Some(&virtual_fd) = kernel_to_vfd_mapping.get(&kernel_index) {
                    // Use fdtables helper to convert kernel fd back to virtual fd
                    if let Some(converted_vfd) = fdtables::convert_poll_result_back_to_virtual(
                        FDKIND_KERNEL, 
                        kernel_pollfd.fd as u64, 
                        &fdtables_mapping_table
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
    let (selectbittables, unparsedtables, mappingtable) = match fdtables::prepare_bitmasks_for_select(
        cageid,
        nfds as u64,
        readfds_ptr.map(|ptr| unsafe { *ptr }),
        writefds_ptr.map(|ptr| unsafe { *ptr }),
        exceptfds_ptr.map(|ptr| unsafe { *ptr }),
        &fdkindset,
    ) {
        Ok(result) => result,
        Err(_) => return syscall_error(Errno::EINVAL, "select_syscall", "Failed to prepare bitmasks"),
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
        libc::timeval { tv_sec: 0, tv_usec: 0 }
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
                if readfds_ptr.is_some() { &mut tmp_readfds as *mut _ } else { std::ptr::null_mut() },
                if writefds_ptr.is_some() { &mut tmp_writefds as *mut _ } else { std::ptr::null_mut() },
                if exceptfds_ptr.is_some() { &mut tmp_errorfds as *mut _ } else { std::ptr::null_mut() },
                if timeout_ptr.is_some() { &mut timeout as *mut _ } else { std::ptr::null_mut() },
            )
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "select_syscall");
        }

        // Check for timeout or successful result
        if ret > 0 || (timeout_ptr.is_some() && start_time.elapsed().as_millis() > (timeout.tv_sec as u128 * 1000 + timeout.tv_usec as u128 / 1000)) {
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
