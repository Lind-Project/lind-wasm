use cage::{readtimer, signal_check_trigger, starttimer, timeout_setup_ms, Duration};
use fdtables;
use fdtables::epoll_event;
use lazy_static::lazy_static;
use libc::*;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use std::{mem, ptr};
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::net_const::{EPOLL_CLOEXEC, EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD};
use sysdefs::constants::FDKIND_KERNEL;
use sysdefs::data::net_struct::SockAddr;
use sysdefs::*;
use typemap::cage_helpers::convert_fd_to_host;
use typemap::datatype_conversion::*;
use typemap::network_helpers::{convert_host_sockaddr, convert_sockpair, copy_out_sockaddr};
use typemap::recvmsg_structs::{GuestIovec, GuestMsghdr};

/// `epoll_ctl` handles registering, modifying, and removing the watch set, while `epoll_wait`
/// simply gathers ready events based on what's already registered and writes them back to the
/// user buffer. Since we currently only have a virtual-FD to kernel-FD mapping, allowing user
/// space to use `events[i].data.fd` directly after `epoll_wait` requires translating kernel-side
/// info back to the virtual side. We'll maintain a global reverse mapping (cage, underfd) to vfd,
/// register entries during `epoll_ctl`, and translate data from underfd to vfd before writing
/// events back in `epoll_wait`.
lazy_static! {
    // A hashmap used to store epoll mapping relationships
    // <virtual_epfd <kernel_fd, virtual_fd>>
    static ref REAL_EPOLL_MAP: Mutex<HashMap<u64, HashMap<i32, u64>>> = Mutex::new(HashMap::new());
}

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
pub extern "C" fn poll_syscall(
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

    // Convert arguments after validation
    let nfds = sc_convert_sysarg_to_usize(nfds_arg, nfds_cageid, cageid);
    let original_timeout = sc_convert_sysarg_to_i32(timeout_arg, timeout_cageid, cageid);

    // Convert pollfd array from user space
    let fds_ptr = sc_convert_buf(fds_arg, fds_cageid, cageid) as *mut libc::pollfd;

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
                // Handle non-kernel FDs
                return syscall_error(Errno::EBADFD, "poll_syscall", "Invalid fdkind");
            }
        }
    }

    // Poll all kernel-backed fds with timeout/signal checking loop
    if !all_kernel_pollfds.is_empty() {
        let start_time = starttimer();
        // Keep track of total duration for our exit check in the poll loop
        let (duration, chunk_timeout) = timeout_setup_ms(original_timeout);

        let ret;
        loop {
            let current_chunk_timeout = if duration == Duration::MAX {
                chunk_timeout
            } else {
                std::cmp::min(
                    chunk_timeout as u128,
                    duration.saturating_sub(readtimer(start_time)).as_millis(),
                ) as i32
            };

            let poll_ret = unsafe {
                libc::poll(
                    all_kernel_pollfds.as_mut_ptr(),
                    all_kernel_pollfds.len() as libc::nfds_t,
                    current_chunk_timeout,
                )
            };

            if poll_ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "poll_syscall");
            }

            // Check for ready FDs or time elapsed is greater than the total duration of the timeout
            if poll_ret > 0 || readtimer(start_time) >= duration {
                ret = poll_ret;
                break;
            }

            // Check for signals that may have interrupted the select operation
            // This implements POSIX signal semantics where poll() should return EINTR
            // if interrupted by a signal before any file descriptors become ready or timeout occurs.
            // The signal checking happens in the retry loop to ensure we don't block indefinitely
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
/// TODO: Implement in-memory FD support for select syscall
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
pub extern "C" fn select_syscall(
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

    // Convert timeval pointer to milliseconds for consistency with poll/epoll_wait timeout handling
    // select takes a timeval* (tv_sec + tv_usec), but poll/epoll_wait use integer milliseconds
    let timeout_ms = if let Some(timeout_ptr) = timeout_ptr {
        let t = unsafe { *timeout_ptr };
        (t.tv_sec as i32 * 1000) + (t.tv_usec as i32 / 1000)
    } else {
        -1 // No timeout (infinite)
    };

    // Always use zero timeout for instant return from select - libc select requires timeval struct format
    let mut zero_timeout = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };

    let start_time = starttimer();
    // Keep track of total timeout duration for exit handling later
    let (duration, chunk_timeout) = timeout_setup_ms(timeout_ms);
    // Convert chunk_timeout (ms) to timeval for select

    let mut ret;
    loop {
        let mut tmp_readfds = real_readfds.clone();
        let mut tmp_writefds = real_writefds.clone();
        let mut tmp_errorfds = real_errorfds.clone();

        let current_chunk_ms = if duration == Duration::MAX {
            chunk_timeout
        } else {
            std::cmp::min(
                chunk_timeout as u128,
                duration.saturating_sub(readtimer(start_time)).as_millis(),
            ) as i32
        };

        let mut current_timeout = libc::timeval {
            tv_sec: 0,
            tv_usec: (current_chunk_ms as i64) * 1000,
        };

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
                    &mut current_timeout as *mut _ // libc select requires timeval struct format
                } else {
                    std::ptr::null_mut()
                },
            )
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "select_syscall");
        }

        // Check for valid return or time elapsed is greater than the total duration of the timeout
        // Since we have this check here for total time elapsed, we can call libc select with a 0 timeout as we do above
        if ret > 0 || readtimer(start_time) >= duration {
            real_readfds = tmp_readfds;
            real_writefds = tmp_writefds;
            real_errorfds = tmp_errorfds;
            break;
        }

        // Check for signals that may have interrupted the select operation
        // This implements POSIX signal semantics where select() should return EINTR
        // if interrupted by a signal before any file descriptors become ready or timeout occurs.
        // The signal checking happens in the retry loop to ensure we don't block indefinitely
        if signal_check_trigger(cageid) {
            return syscall_error(Errno::EINTR, "select_syscall", "interrupted");
        }
    }

    let mut unreal_read = HashSet::new();
    let mut unreal_write = HashSet::new();

    // TODO: Implement in-memory FD checking for select syscall
    // Currently only kernel FDs are supported. In-memory pipes and sockets
    // will require custom polling logic when in-memory system is integrated.

    // Convert kernel FD results back to virtual FDs and subsequently write to user memory
    // This step translates the kernel select() results (which contain kernel FDs) back into
    // virtual FDs that using the mapping table created earlier    let (read_flags, read_result) = fdtables::get_one_virtual_bitmask_from_select_result(
    let (read_flags, read_result) = fdtables::get_one_virtual_bitmask_from_select_result(
        FDKIND_KERNEL,
        realnewnfds as u64,
        Some(real_readfds),
        unreal_read,
        None,
        &mappingtable,
    );

    // Write read FD results back to user memory
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
    // Write write FD results back to user memory
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
    // Write error FD results back to user memory
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
pub extern "C" fn epoll_create_syscall(
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

    // Get the virtual epfd and register to fdtables
    let virtual_epfd = fdtables::epoll_create_empty(cageid, false).unwrap();
    fdtables::epoll_add_underfd(cageid, virtual_epfd, FDKIND_KERNEL, kernel_fd as u64);

    // Return virtual epfd
    virtual_epfd as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_create.2.html
///
/// Linux `epoll_create1()` creates an epoll instance and returns a file descriptor referring to that instance.
/// If flags is 0, then, other than the fact that the obsolete size argument is dropped, epoll_create1() is the same as epoll_create().  
/// The following value can be included in flags to obtain different behavior:

///       EPOLL_CLOEXEC
///              Set the close-on-exec (FD_CLOEXEC) flag on the new file descriptor
///
/// ## Implementation Approach:
/// Creates a new virtual epoll file descriptor that maps to an internal epoll instance using fdtables::epoll_create_empty()
/// and registers it in the file descriptor table. If `EPOLL_CLOEXEC` is set, the resulting virtual epoll file descriptor will be
/// marked close-on-exec (`FD_CLOEXEC`), ensuring that it is automatically closed during an `exec` operation. If invalid flags
/// are passed, then EINVAL error is set and -1 is returned.
///
/// ## Arguments:
///     - cageid: current cage identifier.
///     - flags_arg: Bitmask of flags (only `EPOLL_CLOEXEC` is recognized).
///     - flags_cageid: cage ID for flags_arg validation.
///     - arg2-arg6: unused arguments with their respective cage IDs.
///
/// ## Returns:
///     - positive value: file descriptor for the new epoll instance
///     - negative value: error occurred (errno=EINVAL if invalid flags are passed)

pub extern "C" fn epoll_create1_syscall(
    cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
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
        return syscall_error(Errno::EFAULT, "epoll_create1_syscall", "Invalid Cage ID");
    }

    // Convert size argument
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    //Validates that the flags argument contains only allowed bits (EPOLL_CLOEXEC),
    //returning EINVAL if any unknown flags are detected.
    if (flags & !EPOLL_CLOEXEC) != 0 {
        return syscall_error(Errno::EINVAL, "epoll_create1_syscall", "Invalid flags");
    }
    // Create the kernel epoll instance
    let kernel_fd = unsafe { libc::epoll_create1(flags) };

    if kernel_fd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "epoll_create1_syscall");
    }

    //Checks if `EPOLL_CLOEXEC` is set
    let should_cloexec = (flags & EPOLL_CLOEXEC) != 0;

    // Get the virtual epfd and register to fdtables
    let virtual_epfd = fdtables::epoll_create_empty(cageid, should_cloexec).unwrap();
    fdtables::epoll_add_underfd(cageid, virtual_epfd, FDKIND_KERNEL, kernel_fd as u64);

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
pub extern "C" fn epoll_ctl_syscall(
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
    // Convert arguments
    let op = sc_convert_sysarg_to_i32(op_arg, op_cageid, cageid);
    // Validate unused arguments
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "epoll_ctl_syscall", "Invalid Cage ID");
    }

    // Get the underfd of type FDKIND_KERNEL to the vitual fd
    // Details see documentation on fdtables/epoll_get_underfd_hashmap.md
    let epfd = *fdtables::epoll_get_underfd_hashmap(cageid, epfd_arg)
        .unwrap()
        .get(&FDKIND_KERNEL)
        .unwrap();

    // Validate operation
    if op != EPOLL_CTL_ADD && op != EPOLL_CTL_MOD && op != EPOLL_CTL_DEL {
        return syscall_error(Errno::EINVAL, "epoll_ctl_syscall", "Invalid operation");
    }

    // Translate virtual FDs to kernel FDs. We only need to translate this since this is a
    // normal fd, not epfd
    let wrappedvfd = fdtables::translate_virtual_fd(cageid, fd_arg);
    if wrappedvfd.is_err() {
        return syscall_error(Errno::EBADF, "epoll_ctl_syscall", "Bad File Descriptor");
    }

    let vfd = wrappedvfd.unwrap();

    // Convert epoll_event
    let user_event_opt = match sc_convert_addr_to_epollevent(event_arg, event_cageid, cageid) {
        Ok(p) => Some(p),
        Err(_) => {
            if op == EPOLL_CTL_DEL {
                None
            } else {
                return syscall_error(Errno::EFAULT, "epoll_ctl_syscall", "Invalid address");
            }
        }
    };

    // We intentionally DO NOT overwrite the user's epoll_event inside the guest's
    // linear memory. At this layer we translate the user-visible (virtual) FD into a
    // kernel FD (underfd), which is not visible to user space. Mutating
    // the guest-provided epoll_event->data to hold an underfd would leak a kernel-
    // side detail into user memory
    //
    // Instead, we allocate a host-side (non-linear-memory) epoll_event and populate it
    // with the same 'events' mask but with 'data.u64' set to the kernel FD. For DEL we
    // pass a NULL pointer. This host-only struct is passed to libc::epoll_ctl and lives
    // just long enough for the syscall. User memory remains untouched, and on the way
    // back (epoll_wait) we translate from underfd to vfd using our reverse map so that
    // user space continues to see virtual FDs/cookies only.
    let mut tmp: Option<libc::epoll_event> = user_event_opt.map(|ue| libc::epoll_event {
        events: ue.events,
        u64: vfd.underfd as u64,
    });

    let kernel_epoll_event: *mut libc::epoll_event = tmp
        .as_mut()
        .map(|e| e as *mut libc::epoll_event)
        .unwrap_or(ptr::null_mut());

    // Call actual kernel epoll_ctl
    let ret = unsafe { libc::epoll_ctl(epfd as i32, op, vfd.underfd as i32, kernel_epoll_event) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "epoll_ctl_syscall");
    }

    // Update the virtual list -- but we only handle the non-real fd case
    //  try_epoll_ctl will directly return a real fd in libc case
    //  - maybe we could create a new mapping table to handle the mapping relationship..?
    //      ceate inside the fdtable interface? or we could handle inside rawposix..?

    // Update the mapping table for epoll
    if op == libc::EPOLL_CTL_DEL {
        let mut epollmapping = REAL_EPOLL_MAP.lock();
        if let Some(fdmap) = epollmapping.get_mut(&(epfd)) {
            if fdmap.remove(&(vfd.underfd as i32)).is_some() {
                if fdmap.is_empty() {
                    epollmapping.remove(&(epfd));
                }
                return ret;
            }
        }
    } else {
        let mut epollmapping = REAL_EPOLL_MAP.lock();
        epollmapping
            .entry(epfd)
            .or_insert_with(HashMap::new)
            .insert(vfd.underfd as i32, fd_arg);
        return ret;
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/epoll_wait.2.html
///
/// Linux `epoll_wait()` waits for events on an epoll file descriptor.
///
/// ## Implementation Approach:
///
/// Uses the fdtables infrastructure to get virtual epoll data and handles both kernel-backed
/// and in-memory file descriptors. For kernel FDs, calls libc::epoll_wait() on the underlying
/// kernel epoll FD.
///
/// Implementation follows a 4-step isolation pattern: (1) translate virtual epoll FD to kernel FD,
/// (2) create host-side buffer to prevent kernel writing directly to user memory, (3) call
/// libc::epoll_wait() with timeout/signal handling, (4) translate returned kernel FDs back to
/// virtual FDs before writing to user-space events array. This ensures kernel identifiers never leak
/// into user memory while maintaining proper syscall semantics.
///
/// TODO: Implement in-memory FD support for epoll_wait
/// For in-memory FDs, custom polling logic needs to be implemented when in-memory system
/// is integrated. This will involve checking in-memory pipe/socket readiness and converting
/// results back to virtual FDs. Results are converted back to virtual FDs and written to
/// the user-space events array.
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
pub extern "C" fn epoll_wait_syscall(
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

    // Get the underfd of type FDKIND_KERNEL to the vitual fd
    // Details see documentation on fdtables/epoll_get_underfd_hashmap.md
    let epfd = *fdtables::epoll_get_underfd_hashmap(cageid, epfd_arg)
        .unwrap()
        .get(&FDKIND_KERNEL)
        .unwrap();

    // Convert arguments
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

    // Convert events array from user space
    let mut events_ptr = match sc_convert_addr_to_epollevent(events_arg, events_cageid, cageid) {
        Ok(p) => p,
        Err(e) => return syscall_error(Errno::EFAULT, "epoll_wait_syscall", "Invalid address"),
    };

    // We do not let the kernel write epoll events directly into the guest's
    // linear memory. The kernel reports events using kernel-side identifiers
    // (underfd) in the epoll_event.data field, which are not visible to user space
    // in our model. To preserve isolation and avoid leaking underfd
    // values into guest memory, we allocate a host-side (non-linear-memory) buffer
    // `kernel_events` and pass its pointer to epoll_wait. After epoll_wait returns,
    // we translate each reported underfd back into the corresponding virtual FD
    // using our (cage, underfd) to vfd reverse map, and only then write the
    // translated (events, vfd) into the user-provided events array. In short:
    //   kernel to host buffer (underfd) --> translate to guest buffer (vfd).
    let mut events = unsafe { std::slice::from_raw_parts_mut(events_ptr, maxevents as usize) };

    let mut kernel_events: Vec<libc::epoll_event> = Vec::with_capacity(maxevents as usize);
    // Should always be null value before we call libc::epoll_wait
    kernel_events.push(libc::epoll_event { events: 0, u64: 0 });

    if maxevents != 0 {
        let start_time = starttimer();
        // Keep track of total duration for our exit check in the epoll loop
        let (duration, _) = timeout_setup_ms(timeout);

        // Copy results to the guest's events array *after* translation:
        // - `kernel_events[i].u64` holds an underfd (kernel-only).
        // - We map (cage, underfd) to vfd and store the vfd in the user's event data.
        // - We also copy the event mask verbatim.
        // This ensures guest memory only ever contains guest-visible virtual FDs.
        let mut ret;
        loop {
            ret = unsafe {
                libc::epoll_wait(
                    epfd as i32,
                    kernel_events.as_mut_ptr(),
                    maxevents,
                    0, // Trigger instant return from libc epoll as we check elapsed time from start_time for our timeout handling logic
                )
            };

            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "epoll");
            }

            // check for timeout against total duration or if epoll_wait returned successfully.
            if ret > 0 || readtimer(start_time) >= duration {
                break;
            }

            // Check for signals that may have interrupted the select operation
            // This implements POSIX signal semantics where epoll() should return EINTR
            // if interrupted by a signal before any file descriptors become ready or timeout occurs.
            // The signal checking happens in the retry loop to ensure we don't block indefinitely
            if signal_check_trigger(cageid) {
                return syscall_error(Errno::EINTR, "epoll", "interrupted");
            }
        }
        // Convert back to user's data structure
        // Loop over virtual epollfd to find corresponding mapping relationship between kernel fd and virtual fd
        for i in 0..ret as usize {
            let ret_kernelfd = kernel_events[i].u64;
            let epollmapping = REAL_EPOLL_MAP.lock();
            let ret_virtualfd = epollmapping
                .get(&(epfd))
                .and_then(|kernel_map| kernel_map.get(&(ret_kernelfd as i32)).copied());

            // Write back to user's buffer: store virtual fd in the u64 data field
            events[i].u64 = ret_virtualfd.unwrap() as u64;
            events[i].events = kernel_events[i].events;
        }
        return ret;
    }

    return 0; // Should never reach
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
pub extern "C" fn socket_syscall(
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
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "socket_syscall"
        );
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
    fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, cloexec, 0).unwrap()
        as i32
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
pub extern "C" fn connect_syscall(
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
    let addr = addr_arg as *mut u8;

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "connect_syscall"
        );
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
pub extern "C" fn bind_syscall(
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
    let addr = addr_arg as *mut u8;

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "bind_syscall"
        );
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
pub extern "C" fn listen_syscall(
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
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "listen_syscall"
        );
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
pub extern "C" fn accept_syscall(
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
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let addr = addr_arg as *mut u8;

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "accept_syscall"
        );
    }

    let (finalsockaddr, mut addrlen) = convert_host_sockaddr(addr, addr_cageid, cageid);

    let ret_kernelfd = unsafe { libc::accept(fd, finalsockaddr, &mut addrlen as *mut u32) };

    if ret_kernelfd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "accept");
    }

    // We need to register this new kernel fd in fdtables
    let ret_virtualfd =
        fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ret_kernelfd as u64, false, 0)
            .unwrap();

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
pub extern "C" fn setsockopt_syscall(
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
    let optval = optval_arg as *mut u8;
    let optlen = sc_convert_sysarg_to_u32(optlen_arg, optlen_cageid, cageid);

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "setsockopt_syscall"
        );
    }

    let ret = unsafe { libc::setsockopt(fd, level, optname, optval as *mut c_void, optlen) };

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
pub extern "C" fn shutdown_syscall(
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
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "shutdown_syscall"
        );
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
///
/// This implementation follows the isolation pattern: instead of letting the kernel write
/// directly to guest memory, we use an intermediate host-side buffer. The kernel writes
/// to `sockaddr_storage`, then we use `copy_out_sockaddr()` to translate and copy the
/// result into the guest's SockAddr structure. This ensures proper isolation and allows
/// for any necessary path transformations (e.g., for Unix domain sockets in chroot).
///
/// ## Input:
///     - cageid: identifier of the current cage
///     - fd_arg: virtual file descriptor of the socket
///     - addr_arg: pointer to a SockAddr buffer in user space where the address will be stored
///     - addrlen_arg: pointer to socklen_t in user space (input: buffer size, output: actual size)
///
/// ## Return:
///     - On success: 0
///     - On failure: negative errno indicating the error
pub extern "C" fn getsockname_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    addrlen_arg: u64,
    addrlen_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let user_addr = addr_arg as *mut SockAddr;
    let lenp = addrlen_arg as *mut socklen_t;

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getsockname_syscall"
        );
    }

    if lenp.is_null() {
        return syscall_error(Errno::EFAULT, "getsockname_syscall", "len is null");
    }

    // Read initial length and clamp to storage size
    let mut len: socklen_t = unsafe { *lenp };
    let max_len = mem::size_of::<sockaddr_storage>() as socklen_t;
    if len > max_len {
        len = max_len;
    }

    // Call kernel into temporary buffer (avoid writing into SockAddr directly)
    let mut storage: sockaddr_storage = unsafe { mem::zeroed() };
    let ret = unsafe {
        libc::getsockname(
            fd as i32,
            &mut storage as *mut _ as *mut sockaddr,
            &mut len as *mut socklen_t,
        )
    };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "getsockname");
    }

    // Copy into guest-visible SockAddr wrapper + write back len
    unsafe {
        copy_out_sockaddr(user_addr, lenp, &storage);
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
pub extern "C" fn sendto_syscall(
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
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = buf_arg as *mut u8;
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flag = sc_convert_sysarg_to_i32(flag_arg, flag_cageid, cageid);
    let sockaddr = sockaddr_arg as *mut u8;
    let addrlen = sc_convert_sysarg_to_u32(addrlen_arg, addrlen_cageid, cageid);

    // We do not need to explicitly handle the NULL case in `sendto`,
    // because `convert_host_sockaddr` already returns `(ptr::null_mut(), 0)`
    // when the caller provides no address. In addition, sendto does not
    // modify the `sockaddr` passed in, so the pointer type does not need
    // to be mutable.
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
///     - addr_arg(src_addr): pointer to the source address structure or null
///     - addrlen_arg(addrlen): pointer to the source address length or null
///
/// ## Return:
///     - On success: number of bytes received
///     - On failure: negative errno indicating the error
pub extern "C" fn recvfrom_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    flag_arg: u64,
    flag_cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    addrlen_arg: u64,
    addrlen_cageid: u64,
) -> i32 {
    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    let buf = buf_arg as *mut u8;
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);
    let flag = sc_convert_sysarg_to_i32(flag_arg, flag_cageid, cageid);

    // true means user passed NULL for that pointer
    let addr_nullity = sc_convert_arg_nullity(addr_arg, addr_cageid, cageid);
    let addrlen_nullity = sc_convert_arg_nullity(addrlen_arg, addrlen_cageid, cageid);

    // Case 1: both NULL → caller doesn’t want peer address
    // In this case recvfrom() won’t write to addr/addrlen,
    // so we can pass null pointers directly to libc.
    if addr_nullity && addrlen_nullity {
        let ret = unsafe {
            libc::recvfrom(
                fd,
                buf as *mut c_void,
                buflen,
                flag,
                ptr::null_mut(),
                ptr::null_mut(),
            ) as i32
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }

        return ret;
    }
    // Case 2: both non-NULL → caller wants src_addr + addrlen filled
    else if !(addr_nullity || addrlen_nullity) {
        let addr = addr_arg as *mut SockAddr;

        let mut src_storage: sockaddr_storage = unsafe { mem::zeroed() };
        let mut src_len: socklen_t = unsafe { mem::size_of::<sockaddr_storage>() as socklen_t };
        let ret = unsafe {
            libc::recvfrom(
                fd,
                buf as *mut c_void,
                buflen,
                flag,
                &mut src_storage as *mut _ as *mut sockaddr,
                &mut src_len as *mut socklen_t,
            ) as i32
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "recvfrom");
        }

        // Copy peer address back to user’s src_addr / addrlen
        if ret >= 0 {
            unsafe {
                copy_out_sockaddr(addr, src_len as *mut u32, &src_storage);
            }
        }

        return ret;
    }

    0
}

/// recvmsg syscall: receive message from socket (wasm32 guest to host pointer translation).
/// Reads guest msghdr/iovec (ILP32 32-bit layout), translates pointers to host,
/// calls libc::recvmsg, copies back output fields.
/// Returns: bytes received on success, negative errno on failure.
/// Reference: https://man7.org/linux/man-pages/man2/recvmsg.2.html
pub extern "C" fn recvmsg_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    msg_arg: u64,
    msg_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "recvmsg_syscall", "Invalid Cage ID");
    }

    let fd = convert_fd_to_host(fd_arg, fd_cageid, cageid);
    if fd < 0 {
        let e = -fd;
        return handle_errno(e, "recvmsg");
    }
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    // glibc recvmsg.c passes TRANSLATE_GUEST_POINTER_TO_HOST(msg), so msg_arg is a host pointer
    // to a guest-layout (wasm32/ILP32) msghdr.
    if msg_arg == 0 {
        return syscall_error(Errno::EFAULT, "recvmsg_syscall", "msg is null");
    }
    let msg_ptr = msg_arg as *const GuestMsghdr;
    let guest_msg = unsafe { *msg_ptr };

    let iovlen = guest_msg.msg_iovlen as usize;
    if iovlen == 0 || iovlen > 1024 {
        return syscall_error(Errno::EINVAL, "recvmsg_syscall", "msg_iovlen out of range");
    }

    let iov_array_host = sc_convert_uaddr_to_host(guest_msg.msg_iov as u64, msg_cageid, cageid);
    let guest_iov_ptr = iov_array_host as *const GuestIovec;
    let guest_iov = unsafe { std::slice::from_raw_parts(guest_iov_ptr, iovlen) };

    let mut host_iov: Vec<libc::iovec> = Vec::with_capacity(iovlen);
    for g in guest_iov.iter() {
        let base = if g.iov_len == 0 || g.iov_base == 0 {
            ptr::null_mut()
        } else {
            sc_convert_uaddr_to_host(g.iov_base as u64, msg_cageid, cageid) as *mut c_void
        };
        host_iov.push(libc::iovec {
            iov_base: base,
            iov_len: g.iov_len as usize,
        });
    }

    let msg_name = if guest_msg.msg_name == 0 {
        ptr::null_mut()
    } else {
        sc_convert_uaddr_to_host(guest_msg.msg_name as u64, msg_cageid, cageid) as *mut c_void
    };
    let msg_control = if guest_msg.msg_control == 0 {
        ptr::null_mut()
    } else {
        sc_convert_uaddr_to_host(guest_msg.msg_control as u64, msg_cageid, cageid) as *mut c_void
    };

    let mut host_msg = libc::msghdr {
        msg_name,
        msg_namelen: guest_msg.msg_namelen,
        msg_iov: host_iov.as_mut_ptr(),
        msg_iovlen: iovlen,
        msg_control,
        msg_controllen: guest_msg.msg_controllen as usize,
        msg_flags: 0,
    };

    // Validate iov: need non-null base if iov_len > 0
    if iovlen > 0 && host_iov[0].iov_len > 0 && host_iov[0].iov_base.is_null() {
        return syscall_error(Errno::EFAULT, "recvmsg_syscall", "iov_base null with iov_len > 0");
    }

    let ret = unsafe { libc::recvmsg(fd, &mut host_msg, flags) as i32 };
    let e = get_errno();

    if ret < 0 {
        return handle_errno(e, "recvmsg");
    }

    // Copy back only the output fields glibc expects; iov data was written by kernel into guest
    // memory via the translated iov_base pointers.
    let guest_msg_mut = msg_ptr as *mut GuestMsghdr;
    unsafe {
        (*guest_msg_mut).msg_namelen = host_msg.msg_namelen;
        (*guest_msg_mut).msg_controllen = host_msg.msg_controllen as u32;
        (*guest_msg_mut).msg_flags = host_msg.msg_flags as u32;
    }

    ret
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
pub extern "C" fn gethostname_syscall(
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
    let name = name_arg as *mut u8;
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "gethostname_syscall"
        );
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
pub extern "C" fn getsockopt_syscall(
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
    let optval = sc_convert_uaddr_to_host(optval_arg, optval_cageid, cageid) as *mut c_void;
    let optlen = sc_convert_uaddr_to_host(optlen_arg, optlen_cageid, cageid) as *mut socklen_t;

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !sc_unusedarg(arg6, arg6_cageid) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getsockopt_syscall"
        );
    }

    let ret = unsafe { libc::getsockopt(fd, level, optname, optval, optlen) };
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
pub extern "C" fn getpeername_syscall(
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
    let addr = addr_arg as *mut u8;

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getpeername_syscall"
        );
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
///
/// ## Return:
///     - On success: 0  
///     - On failure: negative errno indicating the error
pub extern "C" fn socketpair_syscall(
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
    let virtual_socket_vector = convert_sockpair(
        virtual_socket_vector_arg,
        virtual_socket_vector_cageid,
        cageid,
    )
    .unwrap();

    // would check when `secure` flag has been set during compilation,
    // no-op by default
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "sockpair_syscall"
        );
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
    let vsv_1 =
        fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ksv_1 as u64, cloexec, 0).unwrap();
    let vsv_2 =
        fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, ksv_2 as u64, cloexec, 0).unwrap();

    // Update virtual socketpair struct
    virtual_socket_vector.sock1 = vsv_1 as i32;
    virtual_socket_vector.sock2 = vsv_2 as i32;
    return 0;
}
