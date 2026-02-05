use cage::{
    get_cage, get_shm_length, is_mmap_error, new_shm_segment, round_up_page, shmat_helper,
    shmdt_helper, MemoryBackingType, VmmapOps, HEAP_ENTRY_INDEX, SHM_METADATA,
};
use dashmap::mapref::entry::Entry::{Occupied, Vacant};
use fdtables;
use libc::c_void;
use std::sync::Arc;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::fs_const::{
    FIOASYNC, FIONBIO, F_GETLK64, F_SETLK64, F_SETLKW64, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE,
    MAP_SHARED, O_CLOEXEC, PAGESHIFT, PAGESIZE, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE,
    SHMMAX, SHMMIN, SHM_DEST, SHM_RDONLY, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO,
};

use sysdefs::constants::lind_platform_const::{FDKIND_KERNEL, MAXFD, UNUSED_ARG, UNUSED_ID};
use sysdefs::constants::sys_const::{DEFAULT_GID, DEFAULT_UID};
use sysdefs::logging::lind_debug_panic;
use typemap::cage_helpers::*;
use typemap::datatype_conversion::*;
use typemap::filesystem_helpers::{convert_fstatdata_to_user, convert_statdata_to_user};
use typemap::path_conversion::*;

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
pub extern "C" fn open_syscall(
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
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "open_syscall"
        );
    }

    // Get the kernel fd first
    let kernel_fd = unsafe { libc::open(path.as_ptr(), oflag, mode) };

    if kernel_fd < 0 {
        return handle_errno(get_errno(), "open_syscall");
    }

    // Check if `O_CLOEXEC` has been est
    let should_cloexec = (oflag & O_CLOEXEC) != 0;

    // Mapping a new virtual fd and set `O_CLOEXEC` flag
    match fdtables::get_unused_virtual_fd(
        cageid,
        FDKIND_KERNEL,
        kernel_fd as u64,
        should_cloexec,
        0,
    ) {
        Ok(vfd) => vfd as i32,
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
///
/// ## Returns:
///     - On success, the number of bytes read is returned (zero indicates end of file).
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn read_syscall(
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
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "read");
    }

    // Convert the user buffer and count.
    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "read_syscall"
        );
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
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error (EBADF, EINTR, EIO).
pub extern "C" fn close_syscall(
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
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "close_syscall"
        );
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

/// Reference to Linux: https://man7.org/linux/man-pages/man2/futex.2.html
///
/// The Linux `futex()` syscall provides a mechanism for fast user-space locking. It allows a process or thread
/// to wait for or wake another process or thread on a shared memory location without invoking heavy kernel-side
/// synchronization primitives unless contention arises. This implementation wraps the futex syscall, allowing
/// direct invocation with the relevant arguments passed from the current cage context.
///
/// Input:
///     - cageid: current cageid
///     - uaddr_arg: pointer to the futex word in user memory
///     - futex_op_arg: operation code indicating futex command type
///     - val_arg: value expected at uaddr or the number of threads to wake
///     - val2_arg: timeout or other auxiliary parameter depending on operation
///     - uaddr2_arg: second address used for requeueing operations
///     - val3_arg: additional value for some futex operations
///
/// ## Returns:
///     - On success: 0 or number of woken threads depending on futex operation
///     - On failure: a negative errno value indicating the syscall error
pub extern "C" fn futex_syscall(
    cageid: u64,
    uaddr_arg: u64,
    uaddr_cageid: u64,
    futex_op_arg: u64,
    futex_op_cageid: u64,
    val_arg: u64,
    val_cageid: u64,
    timeout_arg: u64,
    timeout_cageid: u64,
    uaddr2_arg: u64,
    uaddr2_cageid: u64,
    val3_arg: u64,
    val3_cageid: u64,
) -> i32 {
    let uaddr = uaddr_arg;
    let futex_op = sc_convert_sysarg_to_u32(futex_op_arg, futex_op_cageid, cageid);
    let val = sc_convert_sysarg_to_u32(val_arg, val_cageid, cageid);
    let timeout = timeout_arg;
    let uaddr2 = uaddr2_arg;
    let val3 = sc_convert_sysarg_to_u32(val3_arg, val3_cageid, cageid);

    let ret = unsafe { syscall(SYS_futex, uaddr, futex_op, val, timeout, uaddr2, val3) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "futex");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/write.2.html
///
/// Linux `write()` syscall attempts to write `count` bytes from the buffer pointed to by `buf` to the file associated
/// with the open file descriptor, `fd`. RawPOSIX first converts virtual fd to kernel fd due to the `fdtable` subsystem, second
/// translates the `buf_arg` pointer to actual system pointer
///
/// Input:
///     - cageid: current cageid
///     - vfd_arg: virtual file descriptor, needs to be translated kernel fd for future kernel operation
///     - buf_arg: pointer points to a buffer that stores the data
///     - count_arg: length of the buffer
///
/// ## Returns:
///     - Upon successful completion of this call, we return the number of bytes written. This number will never be greater
///         than `count`. The value returned may be less than `count` if the write_syscall() was interrupted by a signal, or
///         if the file is a pipe or FIFO or special file and has fewer than `count` bytes immediately available for writing.
pub extern "C" fn write_syscall(
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
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "write");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "write_syscall"
        );
    }

    let ret = unsafe { libc::write(kernel_fd, buf as *const c_void, count) as i32 };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "write");
    }
    return ret;
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
pub extern "C" fn mkdir_syscall(
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
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "mkdir_syscall"
        );
    }

    let ret = unsafe { libc::mkdir(path.as_ptr(), mode) };
    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "mkdir");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/pipe.2.html
///
/// Linux `pipe()` syscall is equivalent to calling `pipe2()` with flags set to zero.
/// Call to the kernel here.
///
/// ## Input:
///     - cageid: current cage identifier.
///     - pipefd_arg: a u64 representing the pointer to the PipeArray (user's perspective).
///     - pipefd_cageid: cage identifier for the pointer argument.
///
/// ## Return:
/// On success, zero is returned.  On error, -1 is returned, errno is
/// set to indicate the error, and pipefd is left unchanged.
pub extern "C" fn pipe_syscall(
    cageid: u64,
    pipefd_arg: u64,
    pipefd_cageid: u64,
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
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "pipe_syscall"
        );
    }

    // Convert the u64 pointer into a mutable reference to PipeArray
    let pipefd = match sc_convert_addr_to_pipearray(pipefd_arg, pipefd_cageid, cageid) {
        Ok(p) => p,
        Err(e) => return syscall_error(Errno::EFAULT, "pipe", "Invalid address"),
    };

    // Create an array to hold the two kernel file descriptors
    let mut kernel_fds: [i32; 2] = [0; 2];
    let ret = unsafe { libc::pipe(kernel_fds.as_mut_ptr()) };
    if ret < 0 {
        return handle_errno(get_errno(), "pipe_syscall");
    }

    // Get virtual fd for read end
    let read_vfd = match fdtables::get_unused_virtual_fd(
        cageid,
        FDKIND_KERNEL,
        kernel_fds[0] as u64,
        false,
        0,
    ) {
        Ok(fd) => fd as i32,
        Err(_e) => {
            unsafe {
                libc::close(kernel_fds[0]);
                libc::close(kernel_fds[1]);
            }
            return syscall_error(
                Errno::EMFILE,
                "pipe_syscall",
                "Failed to get virtual file descriptor",
            );
        }
    };

    // Get virtual fd for write end
    let write_vfd = match fdtables::get_unused_virtual_fd(
        cageid,
        FDKIND_KERNEL,
        kernel_fds[1] as u64,
        false,
        0,
    ) {
        Ok(fd) => fd as i32,
        Err(_e) => {
            // close the kernel pipefd if there's an error
            // on getting virtual fd
            unsafe {
                libc::close(kernel_fds[0]);
                libc::close(kernel_fds[1]);
            }
            return syscall_error(
                Errno::EMFILE,
                "pipe_syscall",
                "Failed to get virtual file descriptor",
            );
        }
    };

    // Update PipeArray located in cage linear memory
    pipefd.readfd = read_vfd;
    pipefd.writefd = write_vfd;

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/pipe2.2.html
///
/// Linux `pipe2()` syscall creates a unidirectional data channel and returns two file descriptors,
/// one for reading and one for writing. In our implementation, we first convert the user-supplied
/// pointer to a mutable reference to a PipeArray. Then, we call libc::pipe2() with the provided flags.
/// Finally, we obtain new virtual file descriptors for both ends of the pipe using our fd management
/// subsystem (`fdtables`).
///
/// ## Input:
///     - cageid: current cage identifier.
///     - pipefd_arg: a u64 representing the pointer to the PipeArray (user's perspective).
///     - pipefd_cageid: cage identifier for the pointer argument.
///     - flags_arg: this argument contains flags (e.g., O_CLOEXEC) to be passed to pipe2.
///     - flags_cageid: cage identifier for the flags argument.
///
/// ## Return:
/// On success, zero is returned.  On error, -1 is returned, errno is
/// set to indicate the error, and pipefd is left unchanged.
pub extern "C" fn pipe2_syscall(
    cageid: u64,
    pipefd_arg: u64,
    pipefd_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    // Validate flags (only O_NONBLOCK and O_CLOEXEC are allowed)
    let allowed_flags = fs_const::O_NONBLOCK | fs_const::O_CLOEXEC;
    if flags & !allowed_flags != 0 {
        return syscall_error(Errno::EINVAL, "pipe2_syscall", "Invalid flags");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "pipe2_syscall"
        );
    }
    // Convert the u64 pointer into a mutable reference to PipeArray
    let pipefd = match sc_convert_addr_to_pipearray(pipefd_arg, pipefd_cageid, cageid) {
        Ok(p) => p,
        Err(e) => return syscall_error(Errno::EFAULT, "pipe2", "Invalid address"),
    };
    // Create an array to hold the two kernel file descriptors
    let mut kernel_fds: [i32; 2] = [0; 2];
    let ret = unsafe { libc::pipe2(kernel_fds.as_mut_ptr(), flags) };
    if ret < 0 {
        return handle_errno(get_errno(), "pipe2_syscall");
    }

    // Check whether O_CLOEXEC is set
    let should_cloexec = (flags & fs_const::O_CLOEXEC) != 0;

    // Get virtual fd for read end
    let read_vfd = match fdtables::get_unused_virtual_fd(
        cageid,
        FDKIND_KERNEL,
        kernel_fds[0] as u64,
        should_cloexec,
        0,
    ) {
        Ok(fd) => fd as i32,
        Err(_e) => {
            // close the kernel pipefd if there's an error
            // on getting virtual fd
            unsafe {
                libc::close(kernel_fds[0]);
                libc::close(kernel_fds[1]);
            }
            return syscall_error(
                Errno::EMFILE,
                "pipe2_syscall",
                "Failed to get virtual file descriptor",
            );
        }
    };

    // Get virtual fd for write end
    let write_vfd = match fdtables::get_unused_virtual_fd(
        cageid,
        FDKIND_KERNEL,
        kernel_fds[1] as u64,
        should_cloexec,
        0,
    ) {
        Ok(fd) => fd as i32,
        Err(_e) => {
            unsafe {
                libc::close(kernel_fds[0]);
                libc::close(kernel_fds[1]);
            }
            return syscall_error(
                Errno::EMFILE,
                "pipe2_syscall",
                "Failed to get virtual file descriptor",
            );
        }
    };

    // Update PipeArray located in cage linear memory
    pipefd.readfd = read_vfd;
    pipefd.writefd = write_vfd;

    ret
}

/// Handles the `mmap_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `mmap_syscall` by updating the `vmmap` entries and performing
/// the necessary mmap operations. The handling logic is as follows:
/// 1. Restrict allowed flags to `MAP_FIXED`, `MAP_SHARED`, `MAP_PRIVATE`, and `MAP_ANONYMOUS`.
/// 2. Disallow `PROT_EXEC`; return `EINVAL` if the `prot` argument includes `PROT_EXEC`.
/// 3. If `MAP_FIXED` is not specified, query the `vmmap` structure to locate an available memory region.
///    Otherwise, use the address provided by the user.
/// 4. Invoke the actual `mmap` syscall with the `MAP_FIXED` flag to configure the memory region's protections.
/// 5. Update the corresponding `vmmap` entry.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `mmap` syscall.
/// * `addr` - Starting address of the memory region to mmap.
/// * `len` - Length of the memory region to mmap.
/// * `prot` - Memory protection flags (e.g., `PROT_READ`, `PROT_WRITE`).
/// * `flags` - Mapping flags (e.g., `MAP_SHARED`, `MAP_ANONYMOUS`).
/// * `fildes` - File descriptor associated with the mapping, if applicable.
/// * `off` - Offset within the file, if applicable.
///
/// # Returns
/// * `u32` - Result of the `mmap` operation. See "man mmap" for details
pub extern "C" fn mmap_syscall(
    cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    prot_arg: u64,
    prot_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    off_arg: u64,
    off_cageid: u64,
) -> i32 {
    let mut addr = {
        if addr_arg == 0 {
            0 as *mut u8
        } else {
            sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid)
        }
    };
    let mut len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let mut prot = sc_convert_sysarg_to_i32(prot_arg, prot_cageid, cageid);
    let mut flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let mut fildes = sc_convert_sysarg_to_i32(vfd_arg, vfd_cageid, cageid);
    let mut off = sc_convert_sysarg_to_i64(off_arg, off_cageid, cageid);

    let cage = get_cage(cageid).unwrap();

    let mut maxprot = PROT_READ | PROT_WRITE;

    // Validate flags - only these four flags are supported
    // Note: We explicitly validate rather than silently strip unsupported flags to:
    // 1. Prevent security issues (e.g., MAP_FIXED_NOREPLACE being ignored)
    // 2. Maintain program correctness (e.g., MAP_SHARED_VALIDATE expects validation)
    // 3. Make debugging easier by failing fast rather than having mysterious behavior later
    let allowed_flags =
        MAP_FIXED as i32 | MAP_SHARED as i32 | MAP_PRIVATE as i32 | MAP_ANONYMOUS as i32;
    if flags & !allowed_flags != 0 {
        return syscall_error(Errno::EINVAL, "mmap", "Unsupported mmap flags");
    }

    if prot & PROT_EXEC > 0 {
        lind_debug_panic("mmap protection flag PROT_EXEC is not allowed in Lind");
    }

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64);
    if rounded_addr != addr as u64 {
        return syscall_error(Errno::EINVAL, "mmap", "address it not aligned");
    }

    // offset should be non-negative and multiple of pages
    if off < 0 {
        return syscall_error(Errno::EINVAL, "mmap", "offset cannot be negative");
    }
    let rounded_off = round_up_page(off as u64);
    if rounded_off != off as u64 {
        return syscall_error(Errno::EINVAL, "mmap", "offset it not aligned");
    }

    // round up length to be multiple of pages
    let rounded_length = round_up_page(len as u64);

    let mut useraddr = addr as u32;
    // if MAP_FIXED is not set, then we need to find an address for the user
    if flags & MAP_FIXED as i32 == 0 {
        let mut vmmap = cage.vmmap.write();
        let result;

        // pick an address of appropriate size, anywhere
        if useraddr == 0 {
            result = vmmap.find_map_space(rounded_length as u32 >> PAGESHIFT, 1);
        } else {
            // use address user provided as hint to find address
            result = vmmap.find_map_space_with_hint(
                rounded_length as u32 >> PAGESHIFT,
                1,
                addr as u32 >> PAGESHIFT,
            );
        }

        // did not find desired memory region
        if result.is_none() {
            return syscall_error(Errno::ENOMEM, "mmap", "no memory");
        }

        let space = result.unwrap();
        useraddr = (space.start() << PAGESHIFT) as u32;
    }

    flags |= MAP_FIXED as i32;

    // either MAP_PRIVATE or MAP_SHARED should be set, but not both
    if (flags & MAP_PRIVATE as i32 == 0) == (flags & MAP_SHARED as i32 == 0) {
        return syscall_error(Errno::EINVAL, "mmap", "invalid flags");
    }

    let vmmap = cage.vmmap.read();

    let sysaddr = vmmap.user_to_sys(useraddr);

    drop(vmmap);

    if rounded_length > 0 {
        if flags & MAP_ANONYMOUS as i32 > 0 {
            fildes = -1;
        }

        let result = mmap_inner(
            cageid,
            sysaddr as *mut u8,
            rounded_length as usize,
            prot,
            flags,
            fildes,
            off,
        );

        // Check for error BEFORE sys_to_user conversion
        if is_mmap_error(result) {
            let errno = get_errno();
            return handle_errno(errno, "mmap");
        }

        let vmmap = cage.vmmap.read();
        let result = vmmap.sys_to_user(result);
        drop(vmmap);

        // if mmap addr is positive, that would mean the mapping is successful and we need to update the vmmap entry
        if result >= 0 {
            if result != useraddr {
                panic!("MAP_FIXED not fixed");
            }

            let mut vmmap = cage.vmmap.write();
            let backing = {
                if flags as u32 & MAP_ANONYMOUS > 0 {
                    MemoryBackingType::Anonymous
                } else {
                    // if we are doing file-backed mapping, we need to set maxprot to the file permission
                    let flags = fcntl_syscall(
                        cageid,
                        fildes as u64,
                        vfd_cageid,
                        F_GETFL as u64,
                        flags_cageid,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    );
                    if flags < 0 {
                        return syscall_error(Errno::EINVAL, "mmap", "invalid file descriptor")
                            as i32;
                    }
                    maxprot &= flags;
                    MemoryBackingType::FileDescriptor(fildes as u64)
                }
            };

            // update vmmap entry
            let _ = vmmap.add_entry_with_overwrite(
                useraddr >> PAGESHIFT,
                (rounded_length >> PAGESHIFT) as u32,
                prot,
                maxprot,
                flags,
                backing,
                off,
                len as i64,
                cageid,
            );
        }
    }

    useraddr as i32
}

/// Helper function for `mmap` / `munmap`
///
/// This function calls underlying libc::mmap and serves as helper functions for memory related (vmmap related)
/// syscalls. This function provides fd translation between virtual to kernel.
///
/// Returns:
/// - On success: valid page-aligned memory address
/// - On failure: -1 cast to usize (non-page-aligned, caller should check alignment, get_errno and handle_errno)
/// - On fd translation error: negative errno value cast to usize (non-page-aligned)
pub extern "C" fn mmap_inner(
    cageid: u64,
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    vfd_arg: i32,
    off: i64,
) -> usize {
    if vfd_arg != -1 {
        match fdtables::translate_virtual_fd(cageid, vfd_arg as u64) {
            Ok(kernel_fd) => {
                let ret = unsafe {
                    libc::mmap(
                        addr as *mut c_void,
                        len,
                        prot,
                        flags,
                        kernel_fd.underfd as i32,
                        off,
                    ) as i64
                };

                // Return raw result (including -1 on error)
                // Caller will check page alignment to detect errors
                ret as usize
            }
            Err(_e) => {
                return syscall_error(Errno::EBADF, "mmap", "Bad File Descriptor") as usize;
            }
        }
    } else {
        // Handle mmap with fd = -1 (anonymous memory mapping or special case)
        let ret = unsafe { libc::mmap(addr as *mut c_void, len, prot, flags, -1, off) as i64 };
        // Return raw result (including -1 on error)
        // Caller will check page alignment to detect errors
        ret as usize
    }
}

/// Handler of the `munmap_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `munmap_syscall` by updating the `vmmap` entries and managing
/// the unmap operation. Instead of invoking the actual `munmap` syscall, the unmap operation
/// is simulated by setting the specified region to `PROT_NONE`. The memory remains valid but
/// becomes inaccessible due to the `PROT_NONE` setting.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that calls the `munmap`
/// * `addr` - Starting address of the region to unmap
/// * `length` - Length of the region to unmap
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn munmap_syscall(
    cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
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
    let mut addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "munmap_syscall"
        );
    }

    if len == 0 {
        return syscall_error(Errno::EINVAL, "munmap", "length cannot be zero");
    }
    let cage = get_cage(addr_cageid).unwrap();

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64) as usize;
    if rounded_addr != addr as usize {
        return syscall_error(Errno::EINVAL, "munmap", "address it not aligned");
    }

    let vmmap = cage.vmmap.read();
    let sysaddr = vmmap.user_to_sys(rounded_addr as u32);
    drop(vmmap);

    let rounded_length = round_up_page(len as u64) as usize;

    // we are replacing munmap with mmap because we do not want to really deallocate the memory region
    // we just want to set the prot of the memory region back to PROT_NONE
    let result = unsafe {
        libc::mmap(
            sysaddr as *mut c_void,
            rounded_length,
            PROT_NONE,
            (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32,
            -1,
            0,
        ) as usize
    };
    // Check for different failure modes with specific error messages
    if result as isize == -1 {
        let errno = get_errno();
        panic!(
            "munmap: mmap failed during memory protection reset with errno: {:?}",
            errno
        );
    }

    if result != sysaddr {
        panic!(
            "munmap: MAP_FIXED violation - mmap returned address {:p} but requested {:p}",
            result as *const c_void, sysaddr as *const c_void
        );
    }

    let mut vmmap = cage.vmmap.write();

    vmmap.remove_entry(rounded_addr as u32 >> PAGESHIFT, len as u32 >> PAGESHIFT);

    0
}

/// Handles the `brk_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `brk_syscall` by updating the `vmmap` entries and performing
/// the necessary operations to adjust the program break. Specifically, it updates the program
/// break by modifying the end of the heap entry (the first entry in `vmmap`) and invokes `mmap`
/// to adjust the memory protection as needed.
///
/// Per man page: the actual Linux system call returns the new program
/// break on success.  On failure, the system call returns the current
/// break.  The glibc wrapper function does some work (i.e., checks
/// whether the new break is less than addr) to provide the 0 and -1
/// return values.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `brk` syscall.
/// * `brk` - The new program break address.
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn brk_syscall(
    cageid: u64,
    brk_arg: u64,
    brk_cageid: u64,
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
    let brk = sc_convert_sysarg_to_i32(brk_arg, brk_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "brk_syscall"
        );
    }

    let cage = get_cage(cageid).unwrap();

    let mut vmmap = cage.vmmap.write();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    assert!(heap.npages == vmmap.program_break);

    // passing 0 to brk will always return the current brk
    if brk == 0 {
        return (PAGESIZE * heap.npages) as i32;
    }

    let old_brk_page = heap.npages;
    // round up the break to multiple of pages
    let brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as u32;

    // if we are incrementing program break, we need to check if we have enough space
    if brk_page > old_brk_page {
        if vmmap.check_existing_mapping(old_brk_page, brk_page - old_brk_page, 0) {
            return syscall_error(Errno::ENOMEM, "brk", "no memory");
        }
    }

    // remove the old entries since new entry is overlapping with it.
    vmmap.remove_entry(0, old_brk_page);

    // update vmmap entry
    vmmap.add_entry_with_overwrite(
        0,
        brk_page,
        heap.prot,
        heap.maxprot,
        heap.flags,
        heap.backing,
        heap.file_offset,
        heap.file_size,
        heap.cage_id,
    );

    let old_heap_end_usr = (old_brk_page * PAGESIZE) as u32;
    let old_heap_end_sys = vmmap.user_to_sys(old_heap_end_usr) as *mut u8;

    let new_heap_end_usr = (brk_page * PAGESIZE) as u32;
    let new_heap_end_sys = vmmap.user_to_sys(new_heap_end_usr) as *mut u8;

    vmmap.set_program_break(brk_page);

    drop(vmmap);

    // if new brk is larger than old brk
    // we need to mmap the new region
    if brk_page > old_brk_page {
        let ret = mmap_inner(
            brk_cageid,
            old_heap_end_sys,
            ((brk_page - old_brk_page) * PAGESIZE) as usize,
            heap.prot,
            (heap.flags as u32 | MAP_FIXED) as i32,
            -1,
            0,
        );

        // Check for error using page alignment
        if is_mmap_error(ret) {
            let errno = get_errno();
            return handle_errno(errno, "brk");
        }
    }
    // if we are shrinking the brk
    // we need to do something similar to munmap
    // to unmap the extra memory
    else if brk_page < old_brk_page {
        let ret = mmap_inner(
            brk_cageid,
            new_heap_end_sys,
            ((old_brk_page - brk_page) * PAGESIZE) as usize,
            PROT_NONE,
            (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32,
            -1,
            0,
        );

        // Check for error using page alignment
        if is_mmap_error(ret) {
            let errno = get_errno();
            return handle_errno(errno, "brk");
        }
    }

    // return brk address
    (PAGESIZE * brk_page) as i32
}

//------------------------------------FCNTL SYSCALL------------------------------------
/// This function will be different in new code base (when splitting out type conversion function)
/// since the conversion from u64 -> i32 in negative number will be different. These lines are repeated
/// in 5 out of 6 fcntl_syscall cases, so wrapped these loc into helper functions to make code cleaner.
///
/// ## Arguments
/// cageid: cage ID associate with virtual file descriptor
/// vfd_arg: virtual file descriptor
///
/// ## Return Type
/// On success:
/// Return corresponding FDTableEntry that contains
/// (1) underlying kernel fd.
/// (2) file descriptor kind.
/// (3) O_CLOEXEC flag.
/// (4) file descriptor specific extra information.
///
/// On error:
/// Return error num EBADF(Bad File Descriptor)
pub extern "C" fn _fcntl_helper(cageid: u64, vfd_arg: u64) -> Result<fdtables::FDTableEntry, Errno> {
    if vfd_arg > MAXFD as u64 {
        return Err(Errno::EBADF);
    }
    // Get underlying kernel fd
    let wrappedvfd = fdtables::translate_virtual_fd(cageid, vfd_arg);
    if wrappedvfd.is_err() {
        return Err(Errno::EBADF);
    }
    Ok(wrappedvfd.unwrap())
}

/// Reference: https://man7.org/linux/man-pages/man2/fcntl.2.html
///
/// Due to the design of `fdtables` library, different virtual fds created by `dup`/`dup2` are
/// actually refer to the same underlying kernel fd. Therefore, in `fcntl_syscall` we need to
/// handle the cases of `F_DUPFD`, `F_DUPFD_CLOEXEC`, `F_GETFD`, and `F_SETFD` separately.
///
/// Among these, `F_DUPFD` and `F_DUPFD_CLOEXEC` cannot directly use the `dup_syscall` because,
/// in `fcntl`, the duplicated fd is assigned to the lowest available number starting from `arg`,
/// whereas the `dup_syscall` does not have this restriction and instead assigns the lowest
/// available fd number globally.
///
/// Additionally, `F_DUPFD_CLOEXEC` and `F_SETFD` require updating the fd flag information
/// (`O_CLOEXEC`) in fdtables after modifying the underlying kernel fd.
///
/// For all other command operations, after translating the virtual fd to the corresponding
/// kernel fd, they are redirected to the kernel `fcntl` syscall.
///
/// ## Arguments
/// vfd_arg: virtual file descriptor
/// cmd: The operation
/// arg: an optional third argument.  Whether or not this argument is required is determined by op.  
///
/// ## Returns:
///     - For a successful call, the return value depends on the operation:
///       - `F_DUPFD`: The new file descriptor.
///       - `F_GETFD`: Value of file descriptor flags.
///       - `F_GETFL`: Value of file status flags.
///       - `F_GETLEASE`: Type of lease held on file descriptor.
///       - `F_GETOWN`: Value of file descriptor owner.
///       - `F_GETSIG`: Value of signal sent when read or write becomes possible, or zero for traditional SIGIO behavior.
///       - `F_GETPIPE_SZ`, `F_SETPIPE_SZ`: The pipe capacity.
///       - `F_GET_SEALS`: A bit mask identifying the seals that have been set for the inode referred to by fd.
///       - All other commands: Zero.
///     - On error, -1 is returned and errno is set to indicate the error.
///
/// TODO: `F_GETOWN`, `F_SETOWN`, `F_GETOWN_EX`, `F_SETOWN_EX`, `F_GETSIG`, and `F_SETSIG` are used to manage I/O availability signals.
pub extern "C" fn fcntl_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    cmd_arg: u64,
    cmd_cageid: u64,
    int_arg: u64, // arg3: integer value (for F_DUPFD, F_SETFD, F_GETFL, etc.)
    int_arg_cageid: u64,
    ptr_arg: u64, // arg4: translated host pointer (for F_GETLK, F_SETLK, etc.)
    ptr_arg_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let cmd = sc_convert_sysarg_to_i32(cmd_arg, cmd_cageid, cageid);

    // Convert int value only, we handle the pointer args if it's a lock operation
    let arg = int_arg as i32;

    // Validate unused arguments
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fcntl_syscall"
        );
    }

    match (cmd, arg) {
        // Duplicate the file descriptor `vfd_arg` using the lowest-numbered
        // available file descriptor greater than or equal to `arg`. The operation here
        // is quite similar to `dup_syscall`, for specific operation explanation, see
        // comments on `dup_syscall`.
        (F_DUPFD, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, vfd_arg) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            // Get lowest-numbered available file descriptor greater than or equal to `arg`
            match fdtables::get_unused_virtual_fd_from_startfd(
                cageid,
                vfd.fdkind,
                vfd.underfd,
                false,
                0,
                arg as u64,
            ) {
                Ok(new_vfd) => return new_vfd as i32,
                Err(_) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // As for `F_DUPFD`, but additionally set the close-on-exec flag
        // for the duplicate file descriptor.
        (F_DUPFD_CLOEXEC, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, vfd_arg) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            // Get lowest-numbered available file descriptor greater than or equal to `arg`
            // and set the `O_CLOEXEC` flag
            match fdtables::get_unused_virtual_fd_from_startfd(
                cageid,
                vfd.fdkind,
                vfd.underfd,
                true,
                0,
                arg as u64,
            ) {
                Ok(new_vfd) => return new_vfd as i32,
                Err(_) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // Return (as the function result) the file descriptor flags.
        (F_GETFD, ..) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, vfd_arg) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            return vfd.should_cloexec as i32;
        }
        // Set the file descriptor flags to the value specified by arg.
        (F_SETFD, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, vfd_arg) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            // Set underlying kernel fd flag
            let ret = unsafe { libc::fcntl(vfd.underfd as i32, cmd, arg) };
            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "fcntl");
            }
            // Set virtual fd flag
            let cloexec_flag: bool = arg != 0;
            match fdtables::set_cloexec(cageid, vfd_arg as u64, cloexec_flag) {
                Ok(_) => return 0,
                Err(_e) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // todo: F_GETOWN and F_SETOWN commands are not implemented yet
        (F_GETOWN, ..) => DEFAULT_GID as i32,
        (F_SETOWN, arg) if arg >= 0 => 0,
        _ => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, vfd_arg) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            let is_lock_op = cmd == F_GETLK
                || cmd == F_SETLK
                || cmd == F_SETLKW
                || cmd == F_GETLK64
                || cmd == F_SETLK64
                || cmd == F_SETLKW64;

            let ret = if is_lock_op {
                // Lock operation - use ptr_arg (arg4)
                unsafe { libc::fcntl(vfd.underfd as i32, cmd, ptr_arg as *mut c_void) }
            } else {
                // Other operations - use int_arg (arg3)
                unsafe { libc::fcntl(vfd.underfd as i32, cmd, arg) }
            };

            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "fcntl");
            }
            ret
        }
    }
}

//------------------------------------LINK SYSCALL------------------------------------
/// Reference: https://man7.org/linux/man-pages/man2/link.2.html
///
/// `link_syscall` creates a new link (hard link) to an existing file.
///
/// ## Arguments:
///  - `cageid`: Identifier of the calling Cage (namespace / process-like container).
///  - `oldpath_arg`: Address of the existing pathname in the caller's address space.
///  - `oldpath_cageid`: Cage ID associated with `oldpath_arg`.
///  - `newpath_arg`: Address of the new pathname in the caller's address space.
///  - `newpath_cageid`: Cage ID associated with `newpath_arg`.
///  - `arg3`–`arg6` and their corresponding `_cageid`: Reserved arguments (must be unused).
///
/// ## Implementation Details:
///  - The path arguments are translated from the RawPOSIX perspective into host kernel paths
///    using `sc_convert_path_to_host`, which applies path normalization relative to the cage's CWD.
///  - The unused arguments are validated with `sc_unusedarg`; any unexpected values are treated
///    as a security violation.
///  - The underlying `libc::link()` is invoked with the translated paths.
///  - On failure, `errno` is retrieved via `get_errno()` and normalized through `handle_errno()`.
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn link_syscall(
    cageid: u64,
    oldpath_arg: u64,
    oldpath_cageid: u64,
    newpath_arg: u64,
    newpath_cageid: u64,
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
    let oldpath = sc_convert_path_to_host(oldpath_arg, oldpath_cageid, cageid);
    let newpath = sc_convert_path_to_host(newpath_arg, newpath_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "link_syscall"
        );
    }

    let ret = unsafe { libc::link(oldpath.as_ptr(), newpath.as_ptr()) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "link");
    }
    ret
}

//------------------------------------XSTAT SYSCALL------------------------------------
/// `xstat` retrieves file status information (versioned stat interface).
/// Reference: https://man7.org/linux/man-pages/man2/stat.2.html
///
/// ## Arguments:
///  - `vers`: Version parameter for stat structure compatibility.
///  - `pathname`: Path to the file to get status information for.
///  - `statbuf`: Buffer to store the file status information.
///
/// ## Implementation Details:
///  - The path is converted from the RawPOSIX perspective to the host kernel perspective
///    using `sc_convert_path_to_host`, which handles path normalization relative to the cage's CWD.
///  - The underlying libc::stat() is called and results are copied to the user buffer.
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn stat_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    statbuf_arg: u64,
    statbuf_cageid: u64,
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
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "stat_syscall"
        );
    }

    // Declare statbuf by ourselves
    let mut libc_statbuf: stat = unsafe { std::mem::zeroed() };
    let libcret = unsafe { libc::stat(path.as_ptr(), &mut libc_statbuf) };

    if libcret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "xstat");
    }

    // Convert libc stat to StatData and copy to user buffer
    match sc_convert_addr_to_statdata(statbuf_arg, statbuf_cageid, cageid) {
        Ok(statbuf_addr) => convert_statdata_to_user(statbuf_addr, libc_statbuf),
        Err(e) => return syscall_error(e, "xstat", "Bad address"),
    }

    libcret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/statfs.2.html
///
/// Linux `statfs()` syscall returns information about a mounted filesystem
/// that contains the file or directory specified by `path`.  
/// In RawPOSIX, because each Cage has its own virtualized filesystem view,
/// the path is first translated from the Cage's namespace into the host
/// kernel namespace using `sc_convert_path_to_host`.  
/// After translation, the kernel's `libc::statfs()` is invoked to obtain
/// the filesystem information. The resulting `statfs` structure is then
/// converted into our ABI-stable `FStatData` format and copied into the
/// user-provided buffer in Cage memory.
///
/// ## Input:
/// - `cageid`: Identifier of the current Cage
/// - `path_arg`: Wasm address of the pathname string
/// - `path_cageid`: Cage ID associated with `path_arg`
/// - `statbuf_arg`: Wasm address of the buffer where results will be stored
/// - `statbuf_cageid`: Cage ID associated with `statbuf_arg`
/// - `arg3`–`arg6`: Unused arguments (validated for security)
///
/// ## Return Value:
/// - `0` on success  
/// - `-1` on failure, with `errno` set appropriately
pub extern "C" fn statfs_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    statbuf_arg: u64,
    statbuf_cageid: u64,
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
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "statfs_syscall"
        );
    }

    // Cast directly to libc::statfs and write kernel data into buffer.
    let statbuf_ptr = statbuf_arg as *mut libc::statfs;
    let ret = unsafe { libc::statfs(path.as_ptr(), statbuf_ptr) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "statfs");
    }

    ret
}

//------------------------------------FSYNC SYSCALL------------------------------------
/// `fsync` synchronizes a file's in-core state with storage device.
/// Reference: https://man7.org/linux/man-pages/man2/fsync.2.html
///
/// ## Arguments:
///  - `fd`: File descriptor to synchronize.
///
/// ## Implementation Details:
///  - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
///  - This ensures proper translation between RawPOSIX virtual fds and host kernel fds.
///  - The underlying libc::fsync() is called, which synchronizes both file data and metadata.
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn fsync_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
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
    // Type conversion
    let virtual_fd = sc_convert_sysarg_to_i32(fd_arg, fd_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fsync_syscall"
        );
    }

    let kernel_fd = convert_fd_to_host(virtual_fd as u64, fd_cageid, cageid);
    // convert_fd_to_host returns negative errno values on error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "read");
    }

    let ret = unsafe { libc::fsync(kernel_fd) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "fsync");
    }
    return ret;
}

//------------------------------------FDATASYNC SYSCALL------------------------------------
/// `fdatasync` synchronizes a file's data to storage device (but not metadata).
/// Reference: https://man7.org/linux/man-pages/man2/fdatasync.2.html
///
/// ## Arguments:
///  - `fd`: File descriptor to synchronize.
///
/// ## Implementation Details:
///  - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
///  - This ensures proper translation between RawPOSIX virtual fds and host kernel fds.
///  - The underlying libc::fdatasync() is called, which synchronizes only file data (not metadata
///    like timestamps), making it potentially faster than fsync().
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn fdatasync_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
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
    // Type conversion
    let virtual_fd = sc_convert_sysarg_to_i32(fd_arg, fd_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fdatasync_syscall"
        );
    }

    let kernel_fd = convert_fd_to_host(virtual_fd as u64, fd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "read");
    }

    let ret = unsafe { libc::fdatasync(kernel_fd) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "fdatasync");
    }
    return ret;
}

//------------------------------------SYNC_FILE_RANGE SYSCALL------------------------------------
/// `sync_file_range` synchronizes a specific range of bytes in a file to storage device.
/// Reference: https://man7.org/linux/man-pages/man2/sync_file_range.2.html
///
/// ## Arguments:
///  - `fd`: File descriptor to synchronize.
///  - `offset`: Starting byte offset for the range to sync.
///  - `nbytes`: Number of bytes to synchronize.
///  - `flags`: Flags controlling the synchronization behavior.
///
/// ## Implementation Details:
///  - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
///  - This ensures proper translation between RawPOSIX virtual fds and host kernel fds.
///  - The underlying libc::sync_file_range() is called with the specified byte range and flags.
///  - This is more efficient than fsync() for large files when only a specific range needs syncing.
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn sync_file_range_syscall(
    cageid: u64,
    fd_arg: u64,
    fd_cageid: u64,
    offset_arg: u64,
    offset_cageid: u64,
    nbytes_arg: u64,
    nbytes_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let virtual_fd = sc_convert_sysarg_to_i32(fd_arg, fd_cageid, cageid);
    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);
    let nbytes = sc_convert_sysarg_to_i64(nbytes_arg, nbytes_cageid, cageid);
    let flags = sc_convert_sysarg_to_u32(flags_arg, flags_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "sync_file_range_syscall"
        );
    }

    let kernel_fd = convert_fd_to_host(virtual_fd as u64, fd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "read");
    }

    let ret = unsafe { libc::sync_file_range(kernel_fd, offset, nbytes, flags) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "sync_file_range");
    }
    ret
}

//------------------------------------READLINK & READLINKAT SYSCALL------------------------------------
/// Reference: https://man7.org/linux/man-pages/man2/readlink.2.html
///
/// The return value of the readlink syscall indicates the number of bytes written into the buf and -1 if
/// error. The contents of the buf represent the file path that the symbolic link points to. Since the file
/// path perspectives differ between the user application and the host Linux, the readlink implementation
/// requires handling the paths for both the input passed to the Rust kernel libc and the output buffer
/// returned by the kernel libc.
///
/// For the input path, the transformation is straightforward: we normalize it relative to the cage's CWD
/// to convert the user's relative path into an absolute path within the chroot jail.
/// For the output buffer, we need to first verify whether the path written to buf is an absolute
/// path. If it is not, we prepend the current working directory to make it absolute. The result
/// is then truncated to fit within the user-provided buflen, ensuring compliance with the behavior
/// described in the Linux readlink man page, which states that truncation is performed silently if
/// the buffer is too small.
///
/// ## Input:
/// - `cageid`: Identifier of the current Cage
/// - `path_arg`: Address of the symbolic link pathname in Wasm memory
/// - `path_cageid`: Cage ID associated with `path_arg`
/// - `buf_arg`: Address of the user buffer to store the link target
/// - `buf_cageid`: Cage ID associated with `buf_arg`
/// - `buflen_arg`: Size of the user buffer
/// - `buflen_cageid`: Cage ID associated with `buflen_arg`
/// - `arg4`–`arg6`: Unused arguments (validated for security)
///
/// ## Return:
/// - On success: number of bytes placed in `buf` (not null-terminated)  
/// - On failure: `-1`, with `errno` set appropriately
pub extern "C" fn readlink_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    let buf = buf_arg as *mut u8;
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "readlink_syscall"
        );
    }

    // Call to kernel readlink
    let bytes_written = unsafe { libc::readlink(path.as_ptr(), buf as *mut libc::c_char, buflen) };

    if bytes_written < 0 {
        let errno = get_errno();
        return handle_errno(errno, "readlink");
    }

    bytes_written as i32
}

/// `readlinkat` reads the value of a symbolic link relative to a directory file descriptor.
/// Reference: https://man7.org/linux/man-pages/man2/readlinkat.2.html
///
/// ## Arguments:
///  - `dirfd`: Directory file descriptor. If `AT_FDCWD`, it uses the current working directory.
///  - `pathname`: Path to the symbolic link (relative to dirfd).
///  - `buf`: Buffer to store the link target.
///  - `bufsiz`: Size of the buffer.
///
/// There are two cases:
/// Case 1: When `dirfd` is AT_FDCWD:
///   - The path is converted using `sc_convert_path_to_host` and libc::readlink() is called.
///   - This uses the current working directory as the base for relative paths.
///
/// Case 2: When `dirfd` is not AT_FDCWD:
///   - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
///   - The path is converted using `sc_convert_path_to_host` and libc::readlinkat() is called.
///   - This reads the symlink relative to the specified directory.
///
/// ## Return Value:
///  - Number of bytes placed in `buf` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn readlinkat_syscall(
    cageid: u64,
    dirfd_arg: u64,
    dirfd_cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let virtual_fd = sc_convert_sysarg_to_i32(dirfd_arg, dirfd_cageid, cageid);
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    let buf = sc_convert_to_cchar_mut(buf_arg, buf_cageid, cageid);
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "readlinkat_syscall"
        );
    }

    let ret = if virtual_fd == libc::AT_FDCWD {
        unsafe { libc::readlink(path.as_ptr(), buf, buflen) }
    } else {
        // Case 2: Specific directory fd
        let kernel_fd = convert_fd_to_host(virtual_fd as u64, dirfd_cageid, cageid);
        // Return error
        if kernel_fd < 0 {
            return handle_errno(kernel_fd, "readlinkat");
        }

        let raw_path = match get_cstr(path_arg) {
            Ok(p) => p,
            Err(_) => {
                return syscall_error(
                    Errno::EINVAL,
                    "readlinkat",
                    "invalid  
        path",
                )
            }
        };

        unsafe { libc::readlinkat(kernel_fd, raw_path.as_ptr() as *const c_char, buf, buflen) }
    };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "readlinkat");
    }

    ret as i32
}

//------------------RENAME SYSCALL------------------
/// `rename` changes the name or location of a file.
/// Reference: https://man7.org/linux/man-pages/man2/rename.2.html
///
/// ## Arguments:
///  - `oldpath`: Current path of the file.
///  - `newpath`: New path for the file.
///
/// ## Implementation Details:
///  - Both paths are converted from the RawPOSIX perspective to the host kernel perspective
///    using `sc_convert_path_to_host`, which handles path normalization relative to the cage's CWD.
///  - The underlying libc::rename() is called with both converted paths.
///  - This can move files across directories within the same filesystem.
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn rename_syscall(
    cageid: u64,
    oldpath_arg: u64,
    oldpath_cageid: u64,
    newpath_arg: u64,
    newpath_cageid: u64,
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
    let oldpath = sc_convert_path_to_host(oldpath_arg, oldpath_cageid, cageid);
    let newpath = sc_convert_path_to_host(newpath_arg, newpath_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "rename_syscall"
        );
    }

    let ret = unsafe { libc::rename(oldpath.as_ptr(), newpath.as_ptr()) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "rename");
    }
    ret
}

//------------------------------------UNLINK & UNLINAT SYSCALL------------------------------------
/// `unlink` removes a file from the filesystem.
/// Reference: https://man7.org/linux/man-pages/man2/unlink.2.html
///
/// ## Arguments:
///  - `pathname`: Path to the file to be removed.
///
/// ## Implementation Details:
///  - The path is converted from the RawPOSIX perspective to the host kernel perspective
///    using `sc_convert_path_to_host`, which handles path normalization relative to the cage's CWD.
///  - The underlying libc::unlink() is called with the converted path.
///
/// ## Return Value:
///  - `0` on success.
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn unlink_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
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
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);

    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "unlink_syscall"
        );
    }

    let ret = unsafe { libc::unlink(path.as_ptr()) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "unlink");
    }

    ret
}

/// Reference: https://man7.org/linux/man-pages/man2/unlink.2.html
///
/// `unlinkat` removes a file or directory relative to a directory file descriptor.
///
/// ## Arguments:
/// - `dirfd`: Directory file descriptor. If `AT_FDCWD`, it uses the current working directory.
/// - `pathname`: Path of the file/directory to be removed.
/// - `flags`: Can include `AT_REMOVEDIR` to indicate directory removal.
///
/// There are two cases:
/// Case 1: When `dirfd` is AT_FDCWD:
/// - RawPOSIX maintains its own notion of the current working directory.
/// - We convert the provided relative `pathname` (using `convpath` and `normpath`) into an absolute
///   path relative to the cage's CWD within the chroot jail.
/// - After this conversion, the path is already absolute from the host’s perspective, so `AT_FDCWD`
///   doesn't actually rely on the host’s working directory. This avoids mismatches between RawPOSIX
///   and the host environment.
///
/// Case 2: When `dirfd` is not AT_FDCWD:
/// - We translate the RawPOSIX virtual file descriptor to the corresponding kernel file descriptor.
/// - In this case, we simply create a C string from the provided `pathname` (without further conversion)
///   and let the underlying kernel call resolve the path relative to the directory represented by that fd.
///
/// ## Return Value:
/// - `0` on success.
/// - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn unlinkat_syscall(
    cageid: u64,
    dirfd_arg: u64,
    dirfd_cageid: u64,
    pathname_arg: u64,
    pathname_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let dirfd = sc_convert_sysarg_to_i32(dirfd_arg, dirfd_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    // Validate unused args - this should never fail in correct implementation
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "unlinkat_syscall"
        );
    }

    let mut c_path;
    // Determine the appropriate kernel file descriptor and pathname conversion based on dirfd.
    let kernel_fd = if dirfd == AT_FDCWD {
        // Case 1: When AT_FDCWD is used.
        // Convert the provided pathname from the RawPOSIX working directory (which is different from the host's)
        // into an absolute path within the chroot jail.
        c_path = sc_convert_path_to_host(pathname_arg, pathname_cageid, cageid);
        AT_FDCWD
    } else {
        // Case 2: When a specific directory fd is provided.
        // Translate the virtual file descriptor to the corresponding kernel file descriptor.
        let wrappedvfd = fdtables::translate_virtual_fd(cageid, dirfd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "unlinkat", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        // For this case, we pass the provided pathname directly.
        let pathname = pathname_arg;
        let tmp_cstr = get_cstr(pathname).unwrap();
        c_path = CString::new(tmp_cstr).unwrap();
        vfd.underfd as i32
    };

    // Call the underlying libc::unlinkat() function with the fd and pathname.
    let ret = unsafe { libc::unlinkat(kernel_fd, c_path.as_ptr(), flags) };

    // If the call failed, retrieve and handle the errno
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "unlinkat");
    }
    ret
}

//------------------------------------ACCESS SYSCALL------------------------------------
/// `access` checks whether the calling process can access the file pathname.
/// Reference: https://man7.org/linux/man-pages/man2/access.2.html
/// ## Arguments:
///  - `pathname`: Path to the file to check accessibility.
///  - `mode`: Accessibility check mode (F_OK, R_OK, W_OK, X_OK or combinations).
/// ## Implementation Details:
///  - The path is converted from the RawPOSIX perspective to the host kernel perspective
///    using `sc_convert_path_to_host`, which handles path normalization relative to the cage's CWD.
///  - The mode parameter is passed directly to the underlying libc::access() call.
/// ## Return Value:
///  - `0` on success (file is accessible in the requested mode).
///  - `-1` on failure, with `errno` set appropriately.
pub extern "C" fn access_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    amode_arg: u64,
    amode_cageid: u64,
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
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    let amode = sc_convert_sysarg_to_i32(amode_arg, amode_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "access_syscall"
        );
    }

    let ret = unsafe { libc::access(path.as_ptr(), amode) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "access");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/clock_gettime.2.html
///
/// `clock_gettime_syscall` retrieves the time of the specified clock and
/// stores it in a user-provided `timespec` structure.  
///
/// ## Implementation Details:
/// - The `clockid` argument is converted from the Cage's virtual argument
///   into a host `u32`.
/// - The `tp` pointer (destination for the `timespec` result) is translated
///   from Wasm linear memory into a host address via `sc_convert_addr_to_host`.
/// - Unused arguments `arg3`–`arg6` are validated with `sc_unusedarg`.
/// - The underlying `SYS_clock_gettime` syscall is invoked directly with the
///   converted arguments.
/// - On error, `errno` is retrieved with `get_errno()` and normalized through
///   `handle_errno()`.
///
/// ## Arguments:
/// - `cageid`: Identifier of the calling Cage
/// - `clockid_arg`: The clock to be queried (e.g., `CLOCK_REALTIME`)
/// - `tp_arg`: Address of the user buffer for the result `timespec`
/// - `arg3`–`arg6`: Reserved, must be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on failure, with errno set appropriately.
pub extern "C" fn clock_gettime_syscall(
    cageid: u64,
    clockid_arg: u64,
    clockid_cageid: u64,
    tp_arg: u64,
    tp_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let clockid = sc_convert_sysarg_to_u32(clockid_arg, clockid_cageid, cageid);
    let tp = tp_arg as *mut u8;
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "clock_gettime_syscall"
        );
    }

    let ret = unsafe { syscall(SYS_clock_gettime, clockid, tp) as i32 };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "clock_gettime");
    }

    ret
}

/// Linux Reference: https://man7.org/linux/man-pages/man2/dup.2.html
///
/// Since the two file descriptors refer to the same open file description, they share file offset
/// and file status flags. Then, in RawPOSIX, we mapped duplicated file descriptor to same underlying
/// kernel fd.
///
/// ## Arguments:
/// - `virtual_fd`: virtual file descriptor
///
/// ## Returns:
///     - On success, the new file descriptor is returned.
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn dup_syscall(
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
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "dup_syscall"
        );
    }

    let wrappedvfd = fdtables::translate_virtual_fd(cageid, vfd_arg as u64);
    if wrappedvfd.is_err() {
        return syscall_error(Errno::EBADF, "dup", "Bad File Descriptor");
    }
    let vfd = wrappedvfd.unwrap();
    let ret_kernelfd = unsafe { libc::dup(vfd.underfd as i32) };
    let ret_vfd =
        fdtables::get_unused_virtual_fd(cageid, vfd.fdkind, ret_kernelfd as u64, false, 0).unwrap();
    return ret_vfd as i32;
}

/// dup2() performs the same task as dup(), so we utilize dup() here and mapping underlying kernel
/// fd with specific `new_virutalfd`
///
/// ## Arguments:
/// - `old_virtualfd`: original virtual file descriptor
/// - `new_virtualfd`: specified new virtual file descriptor
///
/// ## Returns:
///     - On success, returns the new file descriptor.
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn dup2_syscall(
    cageid: u64,
    old_vfd_arg: u64,
    old_vfd_cageid: u64,
    new_vfd_arg: u64,
    new_vfd_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "dup2_syscall"
        );
    }

    // Validate both virtual fds
    if old_vfd_arg > MAXFD as u64 || new_vfd_arg > MAXFD as u64 {
        return syscall_error(Errno::EBADF, "dup2", "Bad File Descriptor");
    } else if old_vfd_arg == new_vfd_arg {
        // Does nothing
        return new_vfd_arg as i32;
    }

    // If the file descriptor newfd was previously open, it is closed before being reused; the
    // close is performed silently (i.e., any errors during the close are not reported by dup2()).
    // This step is handled inside `fdtables`
    match fdtables::translate_virtual_fd(cageid, old_vfd_arg) {
        Ok(old_vfd) => {
            // Request another virtual fd to refer to same underlying kernel fd as `virtual_fd`
            // from input.
            // The two file descriptors do not share file descriptor flags (the
            // close-on-exec flag).  The close-on-exec flag (FD_CLOEXEC; see fcntl_syscall())
            // for the duplicate descriptor is off
            let _ = fdtables::get_specific_virtual_fd(
                cageid,
                new_vfd_arg,
                old_vfd.fdkind,
                old_vfd.underfd,
                false,
                old_vfd.perfdinfo,
            )
            .unwrap();

            return new_vfd_arg as i32;
        }
        Err(_e) => {
            return syscall_error(Errno::EBADF, "dup2", "Bad File Descriptor");
        }
    }
}

/// dup3() duplicates `old_virtualfd` to `new_virtualfd`, similar to dup2(),
/// but requires the two descriptors to differ and allows setting FD_CLOEXEC via `flags`.
/// It first calls `dup2_syscall` to copy the file descriptor, then sets the close-on-exec flag if requested.

/// ## Arguments:
/// - `old_virtualfd`: source virtual file descriptor
/// - `new_virtualfd`: target virtual file descriptor
/// - `flags`: must be 0 or O_CLOEXEC
///
/// ## Returns:
///     - On success, returns the new file descriptor.
///     - On error, -1 is returned and errno is set to indicate the error (EBADF or EINVAL).
pub extern "C" fn dup3_syscall(
    cageid: u64,
    old_vfd_arg: u64,
    old_vfd_cageid: u64,
    new_vfd_arg: u64,
    new_vfd_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "dup3_syscall"
        );
    }

    if old_vfd_arg > MAXFD as u64 || new_vfd_arg > MAXFD as u64 {
        return syscall_error(Errno::EBADF, "dup3", "Bad File Descriptor");
    }

    if old_vfd_arg == new_vfd_arg {
        return syscall_error(Errno::EINVAL, "dup3", "oldfd and newfd must be different");
    }

    if flags != 0 && flags != O_CLOEXEC {
        return syscall_error(Errno::EINVAL, "dup3", "Invalid flags");
    }

    let ret = dup2_syscall(
        cageid,
        old_vfd_arg,
        old_vfd_cageid,
        new_vfd_arg,
        new_vfd_cageid,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );
    if ret < 0 {
        return ret;
    }

    if flags == O_CLOEXEC {
        let _ = fdtables::set_cloexec(cageid, new_vfd_arg, true);
    }

    return new_vfd_arg as i32;
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/fchdir.2.html
///
/// Linux `fchdir()` syscall changes the current working directory of the calling process to the
/// directory referred to by the open file descriptor `fd`. Since we implement a file descriptor
/// management subsystem (called `fdtables`), we first translate the virtual file descriptor to the
/// corresponding kernel file descriptor before invoking the kernel's `libc::fchdir()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment referring to a directory
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn fchdir_syscall(
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
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "fchdir");
    }

    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fchdir_syscall"
        );
    }

    let ret = unsafe { libc::fchdir(kernel_fd) };
    if ret < 0 {
        return handle_errno(get_errno(), "fchdir");
    }

    // Update the cage's current working directory
    // We need to get the current working directory from the kernel to update the cage
    let mut cwd_buf = [0u8; PATH_MAX as usize];
    let cwd_ptr = unsafe { libc::getcwd(cwd_buf.as_mut_ptr() as *mut i8, cwd_buf.len()) };
    if !cwd_ptr.is_null() {
        if let Some(cage) = get_cage(cageid) {
            let host_path = unsafe { std::ffi::CStr::from_ptr(cwd_ptr) }.to_string_lossy();
            let user_path = PathBuf::from(host_path.as_ref());
            let mut cwd = cage.cwd.write();
            *cwd = Arc::new(user_path);
        }
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/writev.2.html
///
/// Linux `writev()` syscall performs scatter-gather output by writing data from multiple buffers
/// to a file descriptor in a single operation. Since we implement a file descriptor management
/// subsystem (called `fdtables`), we first translate the virtual file descriptor to the corresponding
/// kernel file descriptor, then translate the iovec array pointer from cage virtual memory to host
/// memory before invoking the kernel's `libc::writev()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - iov_arg: pointer to an array of iovec structures describing the buffers (user's perspective)
///     - iovcnt_arg: number of iovec structures in the array
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - On success, the number of bytes written is returned.
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn writev_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    iov_arg: u64,
    iov_cageid: u64,
    iovcnt_arg: u64,
    iovcnt_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "writev");
    }

    let iovcnt = sc_convert_sysarg_to_i32(iovcnt_arg, iovcnt_cageid, cageid);
    let iov_ptr = sc_convert_buf(iov_arg, iov_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "writev"
        );
    }

    let ret = unsafe { libc::writev(kernel_fd, iov_ptr as *const libc::iovec, iovcnt) as i32 };
    if ret < 0 {
        return handle_errno(get_errno(), "writev");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/fstat.2.html
///
/// Linux `fstat()` syscall retrieves information about the file referred to by the open file descriptor `fd`.
/// Since we implement a file descriptor management subsystem (called `fdtables`), we first translate the virtual
/// file descriptor to the corresponding kernel file descriptor, then call the kernel's `libc::fstat()` function.
/// The returned stat structure is converted to our ABI-stable StatData format and copied to the user's buffer.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - stat_arg: pointer to a stat structure where the file information will be stored (user's perspective)
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn fstat_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    statbuf_arg: u64,
    statbuf_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "fstat");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fstat_syscall"
        );
    }

    // Cast directly to libc::stat and write kernel data into buffer.
    let mut host_stat: libc::stat = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::fstat(kernel_fd, &mut host_stat as *mut libc::stat) };
    if ret < 0 {
        return handle_errno(get_errno(), "fstat");
    }

    // Validate guest buffer range and writability
    match sc_convert_addr_to_statdata(statbuf_arg, statbuf_cageid, cageid) {
        // 3) Populate StatData directly
        Ok(statbuf_addr) => convert_statdata_to_user(statbuf_addr, host_stat),
        Err(e) => return syscall_error(e, "fstat", "Bad address"),
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/ftruncate.2.html
///
/// Linux `ftruncate()` syscall truncates the file referred to by the file descriptor `fd` to be at most
/// `length` bytes in size. Since we implement a file descriptor management subsystem (called `fdtables`),
/// we first translate the virtual file descriptor to the corresponding kernel file descriptor, then convert
/// the length argument from u64 to i64 type before invoking the kernel's `libc::ftruncate()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - length_arg: the desired length in bytes for the file truncation
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn ftruncate_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    length_arg: u64,
    length_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "ftruncate");
    }

    let length = sc_convert_sysarg_to_i64(length_arg, length_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "ftruncate_syscall"
        );
    }

    let ret = unsafe { libc::ftruncate(kernel_fd, length) };
    if ret < 0 {
        return handle_errno(get_errno(), "ftruncate");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/fstatfs.2.html
///
/// Linux `fstatfs()` syscall returns information about a mounted filesystem referred to by the open file
/// descriptor `fd`. Since we implement a file descriptor management subsystem (called `fdtables`), we first
/// translate the virtual file descriptor to the corresponding kernel file descriptor, then call the kernel's
/// `libc::fstatfs()` function. The returned statfs structure is converted to our ABI-stable FSData format
/// and copied to the user's buffer.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - statfs_arg: pointer to a statfs structure where the filesystem information will be stored (user's perspective)
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn fstatfs_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    statfs_arg: u64,
    statfs_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "fstatfs");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fstatfs_syscall"
        );
    }

    // 1) Call host fstatfs into a local host variable
    let mut host_statfs: libc::statfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::fstatfs(kernel_fd, &mut host_statfs) };
    if ret < 0 {
        return handle_errno(get_errno(), "fstatfs");
    }

    // 2) Validate guest buffer range and writability
    match sc_convert_addr_to_fstatdata(statfs_arg, statfs_cageid, cageid) {
        // 3) Populate StatData directly
        Ok(statbuf_addr) => convert_fstatdata_to_user(statbuf_addr, host_statfs),
        Err(e) => return syscall_error(e, "fstatfs", "Bad address"),
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getdents64.2.html
///
/// Linux `getdents()` syscall reads several directory entries from the directory referred to by the open file
/// descriptor `fd` into the buffer pointed to by `dirp`. Since we implement a file descriptor management
/// subsystem (called `fdtables`), we first translate the virtual file descriptor to the corresponding kernel
/// file descriptor, then call the kernel's `getdents64` syscall directly to retrieve directory entries.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment referring to a directory
///     - dirp_arg: pointer to a buffer where the directory entries will be stored (user's perspective)
///     - count_arg: size of the buffer pointed to by dirp
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - On success, the number of bytes read is returned.
///     - On end of directory, 0 is returned.
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn getdents_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    dirp_arg: u64,
    dirp_cageid: u64,
    count_arg: u64,
    count_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "getdents");
    }

    let dirp = sc_convert_buf(dirp_arg, dirp_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getdents_syscall"
        );
    }

    let ret =
        unsafe { libc::syscall(libc::SYS_getdents64 as libc::c_long, kernel_fd, dirp, count) };

    ret as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/lseek.2.html
///
/// Linux `lseek()` syscall repositions the file offset of the open file description associated with the file
/// descriptor `fd` to the argument `offset` according to the directive `whence`. Since we implement a file
/// descriptor management subsystem (called `fdtables`), we first translate the virtual file descriptor to the
/// corresponding kernel file descriptor, then convert the offset and whence parameters from cage memory before
/// invoking the kernel's `libc::lseek()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - offset_arg: the new offset according to the directive whence
///     - whence_arg: how to interpret the offset (SEEK_SET, SEEK_CUR, or SEEK_END)
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - On success, the resulting offset location as measured in bytes from the beginning of the file is returned.
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn lseek_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    offset_arg: u64,
    offset_cageid: u64,
    whence_arg: u64,
    whence_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "lseek");
    }

    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);
    let whence = sc_convert_sysarg_to_i32(whence_arg, whence_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "lseek_syscall"
        );
    }

    let ret = unsafe { libc::lseek(kernel_fd, offset, whence) };
    if ret < 0 {
        return handle_errno(get_errno(), "lseek");
    }

    ret as i32
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/pread.2.html
///
/// Linux `pread()` syscall reads up to `count` bytes from the file descriptor `fd` at the
/// given `offset` without changing the file offset. Since we implement a file descriptor management
/// subsystem (called `fdtables`), we first translate the virtual file descriptor to the corresponding
/// kernel file descriptor, then convert the buffer and offset from cage memory before invoking the
/// kernel's `libc::pread()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - buf_arg: pointer to a buffer where the read data will be stored (user's perspective)
///     - count_arg: the maximum number of bytes to read from the file descriptor
///     - offset_arg: file offset at which the input/output operation takes place
///     - arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - On success, the number of bytes read is returned (zero indicates end of file).
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn pread_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    count_arg: u64,
    count_cageid: u64,
    offset_arg: u64,
    offset_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "pread");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "pread_syscall"
        );
    }

    let ret = unsafe { libc::pread(kernel_fd, buf as *mut c_void, count, offset) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "pread");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/pwrite.2.html
///
/// Linux `pwrite()` syscall writes up to `count` bytes from the buffer pointed to by `buf` to the file
/// associated with the open file descriptor, `fd`, starting at the given `offset` without changing the
/// file offset. Since we implement a file descriptor management subsystem (called `fdtables`), we first
/// translate the virtual file descriptor to the corresponding kernel file descriptor, then convert the
/// buffer, count, and offset from cage memory before invoking the kernel's `libc::pwrite()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - buf_arg: pointer to a buffer that stores the data to be written (user's perspective)
///     - count_arg: the maximum number of bytes to write to the file descriptor
///     - offset_arg: file offset at which the input/output operation takes place
///     - arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - On success, the number of bytes written is returned.
///     - On error, -1 is returned and errno is set to indicate the error.
pub extern "C" fn pwrite_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    count_arg: u64,
    count_cageid: u64,
    offset_arg: u64,
    offset_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "pwrite");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "pwrite_syscall"
        );
    }

    let ret = unsafe { libc::pwrite(kernel_fd, buf as *const c_void, count, offset) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "pwrite");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/chdir.2.html
///
/// Linux `chdir()` syscall changes the current working directory of the calling process to the directory
/// specified by path. Since path seen by user is different from actual path on host, we need to convert
/// the path first. RawPOSIX also updates the cage's current working directory in the cage structure after
/// successfully changing the directory, ensuring cage isolation is maintained.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - path_arg: pointer to a pathname naming the directory (user's perspective)
///     - path_cageid: cage identifier for the path argument
///     - arg2, arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn chdir_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
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
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);

    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "chdir_syscall"
        );
    }

    // Call the kernel chdir function
    let ret = unsafe { libc::chdir(path.as_ptr()) };

    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "chdir");
    }

    // Update the cage's current working directory
    if let Some(cage) = get_cage(cageid) {
        let user_path = PathBuf::from(path.to_string_lossy().as_ref());
        let mut cwd = cage.cwd.write();
        *cwd = Arc::new(user_path);
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/rmdir.2.html
///
/// Linux `rmdir()` syscall removes a directory, which must be empty. Since path seen by user is different
/// from actual path on host, we need to convert the path first. RawPOSIX doesn't have any other operations,
/// so all operations will be handled by host. RawPOSIX does error handling for this syscall.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - path_arg: pointer to a pathname naming the directory to be removed (user's perspective)
///     - path_cageid: cage identifier for the path argument
///     - arg2, arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn rmdir_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
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
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);

    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "rmdir_syscall"
        );
    }

    // Call the kernel rmdir function
    let ret = unsafe { libc::rmdir(path.as_ptr()) };

    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "rmdir");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/chmod.2.html
///
/// Linux `chmod()` syscall changes the permissions of a file. Since path seen by user is different from
/// actual path on host, we need to convert the path first. The mode argument specifies the permissions
/// to be assigned to the file and is passed directly to the kernel after type conversion.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - path_arg: pointer to a pathname naming the file (user's perspective)
///     - path_cageid: cage identifier for the path argument
///     - mode_arg: the new file permissions (user's perspective)
///     - mode_cageid: cage identifier for the mode argument
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn chmod_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
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
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);

    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "chmod_syscall"
        );
    }

    // Call the kernel chmod function
    let ret = unsafe { libc::chmod(path.as_ptr(), mode) };

    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "chmod");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/fchmod.2.html
///
/// Linux `fchmod()` syscall changes the permissions of an open file referred to by file descriptor `fd`.
/// Since we implement a file descriptor management subsystem (called `fdtables`), we first translate the
/// virtual file descriptor to the corresponding kernel file descriptor, then convert the mode from cage
/// memory before invoking the kernel's `libc::fchmod()` function.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - mode_arg: the new file permissions to be applied to the file
///     - mode_cageid: cage identifier for the mode argument
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn fchmod_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
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
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "fchmod");
    }

    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fchmod_syscall"
        );
    }

    let ret = unsafe { libc::fchmod(kernel_fd, mode) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "fchmod");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getcwd.2.html
///
/// `getcwd_syscall` retrieves the current working directory for the calling Cage.
///
/// Unlike directly calling `libc::getcwd`, this implementation uses the Cage's
/// own `cwd` field maintained inside the Cage struct. Each Cage has its own
/// logical working directory, which may differ from the host kernel's notion
/// of the filesystem root. Because of this mismatch between the Cage's root
/// and the kernel root, we cannot delegate to `libc::getcwd` and must instead
/// return the Cage-specific `cwd` value.
///
/// ## Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - buf_arg: pointer to a buffer where the current working directory path will be stored (user's perspective)
///     - size_arg: the size of the buffer in bytes
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - On success, returns 0 after copying the current working directory path to the buffer.
///     - On error, returns -1 and errno is set to indicate the error.
pub extern "C" fn getcwd_syscall(
    cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    size_arg: u64,
    size_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let buf = buf_arg as *mut u8;

    let size = sc_convert_sysarg_to_usize(size_arg, size_cageid, cageid);
    if size == 0 {
        return syscall_error(Errno::EINVAL, "getcwd", "Size cannot be zero");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getcwd_syscall"
        );
    }

    let cage = get_cage(cageid).unwrap();
    let cwd_container = cage.cwd.read();
    let path = cwd_container.to_str().unwrap();
    // The required size includes the null terminator
    let required_size = path.len() + 1;
    if required_size > size as usize {
        return syscall_error(Errno::ERANGE, "getcwd_syscall", "Invalid buffer size");
    }
    unsafe {
        ptr::copy(path.as_ptr(), buf, path.len());
        *buf.add(path.len()) = 0;
    }

    // std::copy guarantees it copies exactly path.len() bytes.
    let bytes_written: i32 = path.len().try_into().unwrap();

    bytes_written
}

/// Truncate a file to a specified length
///
/// ## Arguments:
///     - cageid: current cage identifier
///     - path_arg: pointer to the pathname of the file to truncate
///     - path_cageid: cage identifier for the path argument
///     - length_arg: the new length to truncate the file to
///     - length_cageid: cage identifier for the length argument
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// ## Returns:
///     - 0 on success.
///     - -1 on error, with errno set to indicate the error.
pub extern "C" fn truncate_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    length_arg: u64,
    length_cageid: u64,
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
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "truncate_syscall"
        );
    }

    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    let length = sc_convert_sysarg_to_i64(length_arg, length_cageid, cageid);

    // Call libc truncate
    let ret = unsafe { libc::truncate(path.as_ptr() as *const i8, length) };

    if ret == -1 {
        let errno = get_errno();
        return handle_errno(errno, "truncate");
    }

    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/clock_nanosleep.2.html
///
/// `nanosleep_time64_syscall` suspends execution of the calling Cage for
/// the time interval specified in the requested `timespec` structure.
///
/// ## Implementation Details:
/// - The `req` (requested time) and `rem` (remaining time) pointers are
///   converted from Wasm linear memory to host addresses using
///   `sc_convert_buf`.
/// - Unused arguments `arg5` and `arg6` are validated with `sc_unusedarg`.
/// - The underlying `SYS_clock_nanosleep` syscall is invoked directly.  
/// - On error, `errno` is retrieved and normalized through `handle_errno()`.
///
/// ## Arguments:
/// - `cageid`: Identifier of the calling Cage
/// - `clockid_arg`: The clock against which the sleep interval is measured
/// - `flags_arg`: Flags controlling sleep behavior
/// - `req_arg`: Address of the requested sleep interval (`timespec`)
/// - `rem_arg`: Address of the remaining interval (`timespec`) if interrupted
///
/// ## Returns:
///     - 0 on success.
///     - -1 on failure, with errno set appropriately.
pub extern "C" fn nanosleep_time64_syscall(
    cageid: u64,
    clockid_arg: u64,
    clockid_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    req_arg: u64,
    req_cageid: u64,
    rem_arg: u64,
    rem_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let clockid = sc_convert_sysarg_to_u32(clockid_arg, clockid_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let req = sc_convert_buf(req_arg, req_cageid, cageid);
    let rem = sc_convert_buf(rem_arg, rem_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "nanosleep_time64_syscall"
        );
    }
    let ret = unsafe { syscall(SYS_clock_nanosleep, clockid, flags, req, rem) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "nanosleep");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/mprotect.2.html
///
/// Linux `mprotect()` syscall changes the protection of memory pages. It sets the protection
/// bits for memory pages in the range [addr, addr+len-1] to prot.
///
/// ## Arguments:
///     - cageid: current cage identifier
///     - addr_arg: pointer to the start of the memory region to change protection for
///     - addr_cageid: cage ID for addr argument validation
///     - len_arg: length of the memory region in bytes
///     - len_cageid: cage ID for len argument validation  
///     - prot_arg: new protection flags (PROT_READ, PROT_WRITE, PROT_EXEC, PROT_NONE)
///     - prot_cageid: cage ID for prot argument validation
///     - arg4, arg4_cageid: unused argument and its cage ID
///     - arg5, arg5_cageid: unused argument and its cage ID
///     - arg6, arg6_cageid: unused argument and its cage ID
///
/// ## Returns:
///     - 0 on success
///     - -1 on error with appropriate errno set
pub extern "C" fn mprotect_syscall(
    cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    prot_arg: u64,
    prot_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let addr = addr_arg as *mut u8;
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let prot = sc_convert_sysarg_to_i32(prot_arg, prot_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "mprotect_syscall"
        );
    }

    // Validate protection flags
    let valid_prot = PROT_READ | PROT_WRITE | PROT_EXEC | PROT_NONE;
    if prot & !valid_prot != 0 {
        return syscall_error(Errno::EINVAL, "mprotect", "Invalid protection flags");
    }

    // For security, we don't allow PROT_EXEC in lind-wasm
    if prot & PROT_EXEC != 0 {
        return syscall_error(Errno::EINVAL, "mprotect", "PROT_EXEC is not allowed");
    }

    // Call the kernel mprotect
    let ret = unsafe { libc::mprotect(addr as *mut c_void, len, prot) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "mprotect");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/ioctl.2.html
///
/// Linux `ioctl()` syscall manipulates the underlying device parameters of special files.
/// In particular, many operating characteristics of character special files (e.g., terminals)
/// may be controlled with ioctl() operations. The argument fd must be an open file descriptor.
///
/// ## Arguments:
///     - cageid: current cage identifier
///     - vfd_arg: virtual file descriptor (must be an open file descriptor)
///     - vfd_cageid: cage ID for vfd argument validation
///     - req_arg: device-dependent operation code
///     - req_cageid: cage ID for req argument validation
///     - ptrunion_arg: pointer to memory (untyped pointer, traditionally char *argp)
///     - ptrunion_cageid: cage ID for ptrunion argument validation
///     - arg4, arg4_cageid: unused argument and its cage ID
///     - arg5, arg5_cageid: unused argument and its cage ID
///     - arg6, arg6_cageid: unused argument and its cage ID
///
/// ## Returns:
///     - Usually, on success zero is returned. A few ioctl() operations use the return value
///       as an output parameter and return a nonnegative value on success.
///     - On error, -1 is returned, and errno is set to indicate the error.
pub extern "C" fn ioctl_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    req_arg: u64,
    req_cageid: u64,
    ptrunion_arg: u64,
    ptrunion_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let ptrunion = ptrunion_arg as *mut u8;

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "ioctl_syscall"
        );
    }

    // handle FIOCLEX, set close_on_exec flag for the file descriptor
    if req_arg == FIOCLEX {
        let ret = match fdtables::set_cloexec(cageid, vfd_arg, true) {
            Ok(_) => 0,
            Err(_) => syscall_error(Errno::EBADF, "ioctl", "Bad File Descriptor"),
        };

        return ret;
    }

    // Besides FIOCLEX, we only support FIONBIO and FIOASYNC right now.
    // Return error for unsupported requests.
    if req_arg != FIONBIO as u64 && req_arg != FIOASYNC as u64 as u64 {
        lind_debug_panic("Lind unsupported ioctl request");
    }

    let wrappedvfd = fdtables::translate_virtual_fd(cageid, vfd_arg);
    if wrappedvfd.is_err() {
        return syscall_error(Errno::EBADF, "ioctl", "Bad File Descriptor");
    }

    let vfd = wrappedvfd.unwrap();

    let ret = unsafe { libc::ioctl(vfd.underfd as i32, req_arg, ptrunion as *mut c_void) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "ioctl");
    }
    return ret;
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/flock.2.html
///
/// `Flock()` syscall applies or removes an advisory lock on an open file. We first translate the virtual file descriptor to the
/// corresponding kernel file descriptor, then convert the operation flags from cage memory before invoking
/// the kernel's `libc::flock()` function.
///
/// ## Arguments:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - vfd_cageid: cage ID for vfd argument validation
///     - op_arg: operation flags (LOCK_SH, LOCK_EX, LOCK_UN, optionally ORed with LOCK_NB)
///     - op_cageid: cage ID for op argument validation
///     - arg3, arg3_cageid: unused argument and its cage ID
///     - arg4, arg4_cageid: unused argument and its cage ID
///     - arg5, arg5_cageid: unused argument and its cage ID
///     - arg6, arg6_cageid: unused argument and its cage ID
///
/// ## Returns:
///     - 0 on success
///     - -1 on error, with errno set to indicate the error (EBADF, EINTR, EINVAL, ENOLCK, EWOULDBLOCK)
pub extern "C" fn flock_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    op_arg: u64,
    op_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    // Return error
    if kernel_fd < 0 {
        return handle_errno(kernel_fd, "flock");
    }

    let op = sc_convert_sysarg_to_i32(op_arg, op_cageid, cageid);

    // Validate unused arguments
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "flock_syscall"
        );
    }

    // Validate operation flags
    let valid_ops = libc::LOCK_SH | libc::LOCK_EX | libc::LOCK_UN | libc::LOCK_NB;
    if op & !valid_ops != 0 {
        return syscall_error(Errno::EINVAL, "flock", "Invalid operation flags");
    }

    // Ensure at least one primary operation is specified
    let primary_ops = libc::LOCK_SH | libc::LOCK_EX | libc::LOCK_UN;
    if op & primary_ops == 0 {
        return syscall_error(Errno::EINVAL, "flock", "No primary operation specified");
    }

    let ret = unsafe { libc::flock(kernel_fd, op) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "flock");
    }

    ret
}

/// Linux reference: https://man7.org/linux/man-pages/man2/shmget.2.html
///
/// shmget checks whether a shared memory key already exists, and either returns the existing
/// segment ID or creates a new segment (after validating flags and size) before registering
/// it in the metadata tables.
///
/// 1. **Convert arguments**  
///    - Convert `key_arg`, `size_arg`, and `shmflg_arg` into native types,
///      while validating `arg4`–`arg6` are unused.
///
/// 2. **Special case: IPC_PRIVATE**  
///    - Return `ENOENT` since `IPC_PRIVATE` segments are not supported yet.
///
/// 3. **Check if key exists**  
///    - If the key already exists in `shmkeyidtable`:  
///       - If both `IPC_CREAT` and `IPC_EXCL` are set → error `EEXIST`.  
///       - Else return the existing `shmid`.
///
/// 4. **Key does not exist**  
///    - If `IPC_CREAT` not set → error `ENOENT`.  
///    - Validate `size` against `SHMMIN` and `SHMMAX`.  
///    - Allocate new `shmid` via `new_keyid()`.  
///    - Insert into `shmkeyidtable`.  
///    - Create a new shared memory segment with owner `cageid`, default `uid/gid`,
///      and mode = lowest 9 bits of `shmflg`.  
///    - Insert segment into `shmtable`.
///
/// ## Arguments
/// * `cageid`       – The ID of the calling cage, used for ownership and validation.
/// * `key_arg`      – The key used to identify the shared memory segment (raw u64).
/// * `key_cageid`   – The cage ID associated with `key_arg` for validation.
/// * `size_arg`     – The size (in bytes) of the requested segment (raw u64).
/// * `size_cageid`  – The cage ID associated with `size_arg`.
/// * `shmflg_arg`   – Flags controlling creation, permissions, and behavior (raw u64).
/// * `shmflg_cageid`– The cage ID associated with `shmflg_arg`.
///
/// ## Returns:
/// On success, it returns the segment ID; on failure, it returns an appropriate error code.
pub extern "C" fn shmget_syscall(
    cageid: u64,
    key_arg: u64,
    key_cageid: u64,
    size_arg: u64,
    size_cageid: u64,
    shmflg_arg: u64,
    shmflg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let key = sc_convert_sysarg_to_i32(key_arg, key_cageid, cageid);
    let size = sc_convert_sysarg_to_usize(size_arg, size_cageid, cageid);
    let shmflg = sc_convert_sysarg_to_i32(shmflg_arg, shmflg_cageid, cageid);
    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "shmget_syscall"
        );
    }

    if key == IPC_PRIVATE {
        lind_debug_panic("shmget key IPC_PRIVATE is not allowed in Lind");
    }
    let shmid: i32;
    let metadata = &SHM_METADATA;

    // The size of the segment should be rounded up to a multiple of pages
    let rounded_size = round_up_page(size as u64) as usize;

    match metadata.shmkeyidtable.entry(key) {
        Occupied(occupied) => {
            if (IPC_CREAT | IPC_EXCL) == (shmflg & (IPC_CREAT | IPC_EXCL)) {
                return syscall_error(
                    Errno::EEXIST,
                    "shmget",
                    "key already exists and IPC_CREAT and IPC_EXCL were used",
                );
            }
            shmid = *occupied.get();
        }
        Vacant(vacant) => {
            if 0 == (shmflg & IPC_CREAT) {
                return syscall_error(
                    Errno::ENOENT,
                    "shmget",
                    "tried to use a key that did not exist, and IPC_CREAT was not specified",
                );
            }

            if (rounded_size as u32) < SHMMIN || (rounded_size as u32) > SHMMAX {
                return syscall_error(
                    Errno::EINVAL,
                    "shmget",
                    "Size is less than SHMMIN or more than SHMMAX",
                );
            }

            shmid = metadata.new_keyid();
            vacant.insert(shmid);
            let mode = (shmflg & 0x1FF) as u16; // mode is 9 least signficant bits of shmflag, even if we dont really do anything with them

            let segment = new_shm_segment(
                key,
                rounded_size,
                cageid as u32,
                DEFAULT_UID,
                DEFAULT_GID,
                mode,
            );
            metadata.shmtable.insert(shmid, segment);
        }
    };
    shmid // return the shmid
}

/// Linux reference: https://man7.org/linux/man-pages/man3/shmat.3p.html
///
/// Handles the shmat syscall by mapping shared memory segments into the cage's address space.
/// This function manages the attachment of shared memory segments by updating the cage's vmmap
/// and handling the raw shmat helpers.
///
/// 1) **Parse & validate args**
///    - Decode `shmid`, `shmaddr`, `shmflg`; ensure `arg4..arg6` are truly unused (panic if not).
/// 2) **Resolve access mode**
///    - If `SHM_RDONLY` set → `prot = PROT_READ`; otherwise `prot = PROT_READ|PROT_WRITE`.
/// 3) **Lookup segment length**
///    - `get_shm_length(shmid)` → error `EINVAL` if unknown segment.
/// 4) **Validate alignment & sizes**
///    - `shmaddr` must be page-aligned (error `EINVAL` if not).
///    - Round segment length up to page size.
/// 5) **Choose placement in vmmap**
///    - If `shmaddr == 0` → `find_map_space(pages, align=1)`.
///    - Else           → `find_map_space_with_hint(pages, align=1, hint=shmaddr)`.
///    - No fit → error `ENOMEM`.
/// 6) **Translate to system address**
///    - Convert chosen user address to system address via `vmmap.user_to_sys`.
/// 7) **Perform attach in the backend**
///    - Call `shmat_helper(cageid, sysaddr, shmflg, shmid)`.
///    - Must return the same user address; mismatch to panic.
/// 8) **Record mapping**
///    - Add a `vmmap` entry with `backing = SharedMemory(shmid)`,
///      `prot` and `maxprot = prot`, length in pages, offset 0, `len` as filelen.
///
/// # Arguments
/// * `cageid` - The cage ID that is performing the shmat operation
/// * `addr` - The requested address to attach the shared memory segment (can be null)
/// * `prot` - The memory protection flags for the mapping
/// * `shmflag` - Flags controlling the shared memory attachment behavior
/// * `shmid` - The ID of the shared memory segment to attach
///
/// # Returns
/// * `u32` - The address where the shared memory segment was attached, or an error code
///
/// # Errors
/// * `EINVAL` - If the provided address is not page-aligned
/// * `ENOMEM` - If there is insufficient memory to complete the attachment
pub extern "C" fn shmat_syscall(
    cageid: u64,
    shmid_arg: u64,
    shmid_cageid: u64,
    shmaddr_arg: u64,
    shmaddr_cageid: u64,
    shmflg_arg: u64,
    shmflg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let shmid = sc_convert_sysarg_to_i32(shmid_arg, shmid_cageid, cageid);
    let mut useraddr = {
        if shmaddr_arg == 0 {
            0 as u32
        } else {
            sc_convert_sysarg_to_u32(shmaddr_arg, shmaddr_cageid, cageid)
        }
    };
    let shmflag = sc_convert_sysarg_to_i32(shmflg_arg, shmflg_cageid, cageid);
    let mut prot = 0;
    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "shmat_syscall"
        );
    }

    // Get the cage reference.
    let cage = get_cage(cageid).unwrap();

    // If SHM_RDONLY is set in shmflag, then use read-only protection,
    // otherwise default to read–write.
    prot = if shmflag & SHM_RDONLY != 0 {
        PROT_READ
    } else {
        PROT_READ | PROT_WRITE
    };
    let len = match get_shm_length(shmid) {
        Some(l) => l,
        None => return syscall_error(Errno::EINVAL, "shmat", "invalid shmid"),
    };

    // Check that the provided address is page aligned.
    let rounded_addr = round_up_page(useraddr as u64);
    if rounded_addr != useraddr as u64 {
        return syscall_error(Errno::EINVAL, "shmat", "address is not aligned");
    }

    // Round up the length to a multiple of the page size.
    let rounded_length = round_up_page(len as u64);

    // Initialize the user address from the provided address pointer.
    // If addr is null (0), we need to allocate memory space from the virtual memory map (vmmap).
    let mut vmmap = cage.vmmap.write();
    let result;
    if useraddr == 0 {
        // Allocate a suitable space in the virtual memory map for the shared memory segment
        // based on the rounded length of the segment.
        result = vmmap.find_map_space((rounded_length >> PAGESHIFT) as u32, 1);
    } else {
        // Use the user-specified address as a hint to find an appropriate memory address
        // for the shared memory segment.
        result = vmmap.find_map_space_with_hint(
            rounded_length as u32 >> PAGESHIFT,
            1,
            useraddr as u32 >> PAGESHIFT,
        );
    }
    // drop the write lock of vmmap to avoid deadlock
    drop(vmmap);

    if result.is_none() {
        // If no suitable memory space is found, return an error indicating insufficient memory.
        return syscall_error(Errno::ENOMEM, "shmat", "no memory") as i32;
    }
    let space = result.unwrap();
    // Update the user address to the start of the allocated memory space.
    useraddr = (space.start() << PAGESHIFT) as u32;

    // Convert the user address into a system address.
    // Read the virtual memory map to access the user address space.
    let vmmap = cage.vmmap.read();
    // Convert the user address to the corresponding system address for the shared memory segment.
    let sysaddr = vmmap.user_to_sys(useraddr);
    // Release the lock on the virtual memory map as we no longer need it.
    drop(vmmap);

    // Call the raw shmat helper to attach the shared memory segment.
    let result = shmat_helper(cageid, sysaddr as *mut u8, shmflag, shmid);

    // Check for error BEFORE sys_to_user conversion
    if is_mmap_error(result) {
        let errno = get_errno();
        return handle_errno(errno, "shmat");
    }

    let vmmap = cage.vmmap.read();
    let result = vmmap.sys_to_user(result);
    drop(vmmap);

    // If the syscall succeeded, update the vmmap entry.
    if result as i32 >= 0 {
        // Ensure the syscall attached the segment at the expected address.
        if result as u32 != useraddr {
            panic!("shmat did not attach at the expected address");
        }
        let mut vmmap = cage.vmmap.write();
        let backing = MemoryBackingType::SharedMemory(shmid as u64);
        // Use the effective protection (prot) for both the current and maximum protection.
        let maxprot = prot;
        // Add a new vmmap entry for the shared memory segment.
        // Since shared memory is not file-backed, there are no extra mapping flags
        // or file offset parameters to consider; thus, we pass 0 for both.
        vmmap
            .add_entry_with_overwrite(
                useraddr >> PAGESHIFT,
                (rounded_length >> PAGESHIFT) as u32,
                prot,
                maxprot,
                0, // No flags for shared memory mapping
                backing,
                0, // Offset is not applicable for shared memory
                len as i64,
                cageid,
            )
            .expect("shmat: failed to add vmmap entry");
    } else {
        // If the syscall failed, propagate the error.
        return result as i32;
    }

    useraddr as i32
}

/// Linux reference: https://man7.org/linux/man-pages/man3/shmdt.3p.html
///
/// `shmdt_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `shmdt_syscall` by updating the `vmmap` entries and managing
/// the shared memory detachment operation. It performs address validation, converts user
/// addresses to system addresses, and updates the virtual memory mappings accordingly.
///
/// 1) **Parse & validate args**
///    - Decode `shmaddr`; ensure `arg2..arg6` are unused (panic if not).
/// 2) **Validate alignment**
///    - `shmaddr` must be page-aligned (error `EINVAL` if not).
/// 3) **Translate to system address**
///    - `vmmap.user_to_sys(shmaddr)` to get the underlying system pointer.
/// 4) **Perform detach in the backend**
///    - `shmdt_helper(cageid, sysaddr)` → returns detached length (bytes) or negative errno.
/// 5) **Remove mapping from vmmap**
///    - Remove `length >> PAGESHIFT` pages starting at `shmaddr >> PAGESHIFT`.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that calls the `shmdt`
/// * `addr` - Starting address of the shared memory region to detach
///
/// # Returns
/// * `i32` - 0 for success and negative errno for failure
pub extern "C" fn shmdt_syscall(
    cageid: u64,
    shmaddr_arg: u64,
    shmaddr_cageid: u64,
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
    let useraddr = sc_convert_sysarg_to_u32(shmaddr_arg, shmaddr_cageid, cageid);
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "shmdt_syscall"
        );
    }

    // Retrieve the cage reference.
    let cage = get_cage(cageid).unwrap();

    // Check that the provided address is aligned on a page boundary.
    let rounded_addr = round_up_page(useraddr as u64) as usize;
    if rounded_addr != useraddr as usize {
        return syscall_error(Errno::EINVAL, "shmdt", "address is not aligned");
    }

    // Convert the user address into a system address using the vmmap.
    let vmmap = cage.vmmap.read();
    let sysaddr = vmmap.user_to_sys(rounded_addr as u32);
    drop(vmmap);

    // Call shmdt_helper which returns length of the detached segment
    let length = shmdt_helper(cageid, sysaddr as *mut u8);
    if length < 0 {
        return length;
    }

    // Remove the mapping from the vmmap.
    // This call removes the range starting at the page-aligned user address,
    // for the number of pages that cover the shared memory region.
    let mut vmmap = cage.vmmap.write();
    vmmap
        .remove_entry(
            rounded_addr as u32 >> PAGESHIFT,
            (length as u32) >> PAGESHIFT,
        )
        .expect("shmdt: remove_entry failed");

    0
}

/// Linux reference: https://man7.org/linux/man-pages/man3/shmctl.3p.html
///
/// It converts and validates the `shmid`, `cmd`, and optional `buf` arguments, enforces that unused
/// arguments are truly unused (panicking on unexpected values), and then applies
/// the requested control operation: `IPC_STAT` copies the segment’s metadata
/// (`shminfo`) into the caller-provided buffer, and `IPC_RMID` marks the segment
/// for removal (setting `SHM_DEST`) and deletes it immediately if there are no
/// attachments, also clearing the key to id mapping.
///
/// 1) **Parse & validate args**
///    - Decode `shmid`, `cmd`, and optional `buf` pointer; ensure `arg4..arg6` unused (panic if not).
/// 2) **Locate segment**
///    - Lookup `shmid` in `shmtable`; if not found → error `EINVAL`.
/// 3) **Dispatch by `cmd`**
///    - `IPC_STAT`: copy `segment.shminfo` into caller’s `*buf`.
///    - `IPC_RMID`: mark for removal (`segment.rmid = true`, set `SHM_DEST` in mode).
///        * If `shm_nattch == 0`, remove the segment immediately and clear the key→id mapping.
///    - Otherwise: error `EINVAL` (unsupported command).
///
/// ## Arguments
/// * `cageid`        – The ID of the calling cage, used for ownership and validation.
/// * `shmid_arg`     – Shared memory segment ID (raw u64).
/// * `shmid_cageid`  – Cage ID associated with `shmid_arg`, used for validation.
/// * `cmd_arg`       – Control command (e.g., `IPC_STAT`, `IPC_RMID`) (raw u64).
/// * `cmd_cageid`    – Cage ID associated with `cmd_arg`.
/// * `buf_arg`       – Pointer to a buffer (struct shmid_ds) used for returning or
///                     updating segment info, depending on the command.
/// * `buf_cageid`    – Cage ID associated with `buf_arg`.
///
/// ## Returns:
/// On invalid identifiers or
/// unsupported commands, it returns `-EINVAL` via `syscall_error`; on success,
/// returns `0`.
pub extern "C" fn shmctl_syscall(
    cageid: u64,
    shmid_arg: u64,
    shmid_cageid: u64,
    cmd_arg: u64,
    cmd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let shmid = sc_convert_sysarg_to_i32(shmid_arg, shmid_cageid, cageid);
    let cmd = sc_convert_sysarg_to_i32(cmd_arg, cmd_cageid, cageid);
    let buf = sc_convert_addr_to_shmidstruct(buf_arg, buf_cageid, cageid).unwrap();

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "shmctl_syscall"
        );
    }

    let metadata = &SHM_METADATA;

    if let Some(mut segment) = metadata.shmtable.get_mut(&shmid) {
        match cmd {
            IPC_STAT => {
                *buf = segment.shminfo;
            }
            IPC_RMID => {
                segment.rmid = true;
                segment.shminfo.shm_perm.mode |= SHM_DEST as u16;
                if segment.shminfo.shm_nattch == 0 {
                    let key = segment.key;
                    drop(segment);
                    metadata.shmtable.remove(&shmid);
                    metadata.shmkeyidtable.remove(&key);
                }
            }
            _ => {
                return syscall_error(
                    Errno::EINVAL,
                    "shmctl",
                    "Arguments provided do not match implemented parameters",
                );
            }
        }
    } else {
        return syscall_error(Errno::EINVAL, "shmctl", "Invalid identifier");
    }

    0 //shmctl has succeeded!
}

/// Linux reference: https://man7.org/linux/man-pages/man2/getrandom.2.html
///
/// Implements the `getrandom(2)` syscall for a cage. This wrapper converts and
/// validates all caller-provided arguments, resolves the user buffer pointer into
/// a host pointer, enforces cage-ownership consistency, and then directly invokes
/// the host kernel’s `SYS_getrandom` via `syscall()`. Any error from the host is
/// converted into a cage-appropriate errno using `handle_errno`.
///
/// 1) **Parse & validate args**
///    - `buf_arg` is interpreted as a user pointer and converted to a host pointer
///      using `sc_convert_uaddr_to_host`, ensuring it belongs to `cageid`.
///    - `buflen_arg` and `flags_arg` are converted to 32-bit values via
///      `sc_convert_sysarg_to_u32`, validating cage ownership and rejecting
///      malformed arguments.
///    - Unused arguments `arg4..arg6` must be zero; if not, they cause a panic
///      (enforcing a strict syscall ABI).
///
/// 2) **Invoke host syscall**
///    - Calls `syscall(SYS_getrandom, buf, buflen, flags)` unsafely to request
///      random bytes from the host kernel.
///    - On negative return, retrieves `errno` using `get_errno()` and converts it
///      to a standardized cage-side error with `handle_errno`.
///
/// 3) **Return value**
///    - On success, returns the number of random bytes written (0 ≤ n ≤ buflen).
///    - On failure, returns a negative errno (`-EINTR`, `-EAGAIN`, `-EINVAL`, etc.).
///
/// ## Arguments
/// * `cageid`            – Cage issuing the syscall; used to validate all arguments.
/// * `buf_arg`           – Raw user pointer to the destination buffer.
/// * `buf_arg_cageid`    – Cage ID associated with `buf_arg`.
/// * `buflen_arg`        – Number of random bytes requested (raw u64).
/// * `buflen_arg_cageid` – Cage ID associated with `buflen_arg`.
/// * `flags_arg`         – `getrandom` flags (e.g., `GRND_NONBLOCK`) as raw u64.
/// * `flags_arg_cageid`  – Cage ID associated with `flags_arg`.
/// * `arg4..arg6`        – Unused placeholder arguments for syscall ABI; must be zero.
/// * `arg4_cageid..arg6_cageid` – Cage IDs for unused arguments; checked but unused.
///
/// ## Returns
/// On success: number of bytes written (positive `i32`).  
/// On failure: a negative errno from `handle_errno`.
pub extern "C" fn getrandom_syscall(
    cageid: u64,
    buf_arg: u64,
    buf_arg_cageid: u64,
    buflen_arg: u64,
    buflen_arg_cageid: u64,
    flags_arg: u64,
    flags_arg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let buf = buf_arg;
    let buflen = sc_convert_sysarg_to_u32(buflen_arg, buflen_arg_cageid, cageid);
    let flags = sc_convert_sysarg_to_u32(flags_arg, flags_arg_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "getrandom_syscall"
        );
    }

    let ret = unsafe { getrandom(buf as *mut c_void, buflen.try_into().unwrap(), flags) };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "getrandom");
    }

    // convert isize to i32 safely, as ret shouldn't be larger than 32-bit
    // due to buflen being u32
    ret.try_into().unwrap()
}