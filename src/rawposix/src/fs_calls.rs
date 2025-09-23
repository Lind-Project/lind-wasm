use libc::c_void;
// Updated imports - using path_conversion for filesystem operations
use typemap::datatype_conversion::{sc_convert_statdata, sc_convert_fsdata, *};
use sysdefs::data::fs_struct::{StatData, FSData};
use typemap::path_conversion::*;
use sysdefs::constants::err_const::{syscall_error, Errno, get_errno, handle_errno};
use sysdefs::constants::fs_const::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO, O_CLOEXEC, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE, PAGESHIFT, PAGESIZE, SEEK_SET, SEEK_CUR, SEEK_END};
use sysdefs::constants::lind_platform_const::{FDKIND_KERNEL, MAXFD, PATH_MAX};
use sysdefs::constants::sys_const::{DEFAULT_UID, DEFAULT_GID};
use typemap::cage_helpers::*;
use cage::{round_up_page, get_cage, HEAP_ENTRY_INDEX, MemoryBackingType, VmmapOps, check_addr};
use fdtables;
use std::path::PathBuf;
use std::sync::Arc;


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
        return syscall_error(Errno::EFAULT, "open_syscall", "Invalid Cage ID");
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "read", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
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
/// Return:
///     - On success: 0 or number of woken threads depending on futex operation
///     - On failure: a negative errno value indicating the syscall error
pub fn futex_syscall(
    cageid: u64,
    uaddr_arg: u64,
    uaddr_cageid: u64,
    futex_op_arg: u64,
    futex_op_cageid: u64,
    val_arg: u64,
    val_cageid: u64,
    val2_arg: u64,
    val2_cageid: u64,
    uaddr2_arg: u64,
    uaddr2_cageid: u64,
    val3_arg: u64,
    val3_cageid: u64,
) -> i32{
    let uaddr = sc_convert_uaddr_to_host(uaddr_arg, uaddr_cageid, cageid);
    let futex_op = sc_convert_sysarg_to_u32(futex_op_arg, futex_op_cageid, cageid);
    let val = sc_convert_sysarg_to_u32(val_arg, val_cageid, cageid);
    let val2 = sc_convert_sysarg_to_u32(val2_arg, val2_cageid, cageid);
    let uaddr2 = sc_convert_sysarg_to_u32(uaddr2_arg, uaddr2_cageid, cageid);
    let val3 = sc_convert_sysarg_to_u32(val3_arg, val3_cageid, cageid);

    let ret = unsafe { syscall(SYS_futex, uaddr, futex_op, val, val2, uaddr2, val3)  as i32 };
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
/// Output:
///     - Upon successful completion of this call, we return the number of bytes written. This number will never be greater
///         than `count`. The value returned may be less than `count` if the write_syscall() was interrupted by a signal, or
///         if the file is a pipe or FIFO or special file and has fewer than `count` bytes immediately available for writing.
pub fn write_syscall(
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

    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "write", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "write", "Bad File Descriptor");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "write", "Invalid Cage ID");
    }

    // Early return
    if count == 0 {
        return 0;
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
        return syscall_error(Errno::EFAULT, "mkdir_syscall", "Invalid Cage ID");
    }

    let ret = unsafe { libc::mkdir(path.as_ptr(), mode) };
    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "mkdir");
    }
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
pub fn mmap_syscall(
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
    let mut addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);
    let mut len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let mut prot = sc_convert_sysarg_to_i32(prot_arg, prot_cageid, cageid);
    let mut flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let mut fildes = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
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
        return syscall_error(Errno::EINVAL, "mmap", "PROT_EXEC is not allowed");
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
            result =
                vmmap.find_map_space_with_hint(rounded_length as u32 >> PAGESHIFT, 1, addr as u32);
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
/// syscalls. This function provides fd translation between virtual to kernel and error handling.
pub fn mmap_inner(
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

                // Check if mmap failed and return the appropriate error if so
                if ret == -1 {
                    return syscall_error(Errno::EINVAL, "mmap", "mmap failed with invalid flags")
                        as usize;
                }

                ret as usize
            }
            Err(_e) => {
                return syscall_error(Errno::EBADF, "mmap", "Bad File Descriptor") as usize;
            }
        }
    } else {
        // Handle mmap with fd = -1 (anonymous memory mapping or special case)
        let ret = unsafe { libc::mmap(addr as *mut c_void, len, prot, flags, -1, off) as i64 };
        // Check if mmap failed and return the appropriate error if so
        if ret == -1 {
            let errno = get_errno();
            return handle_errno(errno, "mmap") as usize;
        }

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
/// # Returns
/// * `i32` - 0 for success and -1 for failure
pub fn munmap_syscall(
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
        return syscall_error(Errno::EFAULT, "munmap", "Invalid Cage ID");
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
        panic!("munmap: mmap failed during memory protection reset with errno: {:?}", errno);
    }
    
    if result != sysaddr {
        panic!("munmap: MAP_FIXED violation - mmap returned address {:p} but requested {:p}", 
               result as *const c_void, sysaddr as *const c_void);
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
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `brk` syscall.
/// * `brk` - The new program break address.
///
/// # Returns
/// * `u32` - Returns `0` on success or `-1` on failure.
///
pub fn brk_syscall(
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
        return syscall_error(Errno::EFAULT, "brk", "Invalid Cage ID");
    }

    let cage = get_cage(cageid).unwrap();

    let mut vmmap = cage.vmmap.write();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    assert!(heap.npages == vmmap.program_break);

    let old_brk_page = heap.npages;
    // round up the break to multiple of pages
    let brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as u32;

    // if we are incrementing program break, we need to check if we have enough space
    if brk_page > old_brk_page {
        if vmmap.check_existing_mapping(old_brk_page, brk_page - old_brk_page, 0) {
            return syscall_error(Errno::ENOMEM, "brk", "no memory");
        }
    }

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

        if ret < 0 {
            panic!("brk mmap failed");
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

        if ret < 0 {
            panic!("brk mmap failed");
        }
    }

    0
}

/// Handles the `sbrk_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `sbrk_syscall` by updating the `vmmap` entries and managing
/// the program break. It calculates the target program break after applying the specified
/// increment and delegates further processing to the `brk_handler`.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `sbrk` syscall.
/// * `brk` - Increment to adjust the program break, which can be negative.
///
/// # Returns
/// * `u32` - Result of the `sbrk` operation. Refer to `man sbrk` for details.
pub fn sbrk_syscall(
    cageid: u64,
    sbrk_arg: u64,
    sbrk_cageid: u64,
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
    let brk = sc_convert_sysarg_to_i32(sbrk_arg, sbrk_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "sbrk_syscall", "Invalid Cage ID");
    }

    let cage = get_cage(sbrk_cageid).unwrap();

    // get the heap entry
    let mut vmmap = cage.vmmap.read();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    // program break should always be the same as the heap entry end
    assert!(heap.npages == vmmap.program_break);

    // pass 0 to sbrk will just return the current brk
    if brk == 0 {
        return (PAGESIZE * heap.npages) as i32;
    }

    // round up the break to multiple of pages
    // brk increment could possibly be negative
    let brk_page;
    if brk < 0 {
        brk_page = -((round_up_page(-brk as u64) >> PAGESHIFT) as i32);
    } else {
        brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as i32;
    }

    // drop the vmmap so that brk_handler will not deadlock
    drop(vmmap);

    if brk_syscall(
        cageid,
        ((heap.npages as i32 + brk_page) << PAGESHIFT) as u64,
        sbrk_cageid,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ) < 0
    {
        return syscall_error(Errno::ENOMEM, "sbrk", "no memory") as i32;
    }

    // sbrk syscall should return previous brk address before increment
    (PAGESIZE * heap.npages) as i32
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
pub fn _fcntl_helper(cageid: u64, vfd_arg: u64) -> Result<fdtables::FDTableEntry, Errno> {
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
/// ## Return Type
/// The return value is related to the operation determined by `cmd` argument.
///
/// For a successful call, the return value depends on the operation:
/// `F_DUPFD`: The new file descriptor.
/// `F_GETFD`: Value of file descriptor flags.
/// `F_GETFL`: Value of file status flags.
/// `F_GETLEASE`: Type of lease held on file descriptor.
/// `F_GETOWN`: Value of file descriptor owner.
/// `F_GETSIG`: Value of signal sent when read or write becomes possible, or zero for traditional SIGIO behavior.
/// `F_GETPIPE_SZ`, `F_SETPIPE_SZ`: The pipe capacity.
/// `F_GET_SEALS`: A bit mask identifying the seals that have been set for the inode referred to by fd.
/// All other commands: Zero.
/// On error, -1 is returned
///
/// TODO: `F_GETOWN`, `F_SETOWN`, `F_GETOWN_EX`, `F_SETOWN_EX`, `F_GETSIG`, and `F_SETSIG` are used to manage I/O availability signals.
pub fn fcntl_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    cmd_arg: u64,
    cmd_cageid: u64,
    arg_arg: u64,
    arg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let cmd = sc_convert_sysarg_to_i32(cmd_arg, cmd_cageid, cageid);
    let arg = sc_convert_sysarg_to_i32(arg_arg, arg_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fcntl_syscall", "Invalid Cage ID");
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
            let ret = unsafe { libc::fcntl(vfd.underfd as i32, cmd, arg) };
            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "fcntl");
            }
            ret
        }
    }
}

pub fn clock_gettime_syscall(
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
    // let tp = sc_convert_sysarg_to_usize(tp_arg, tp_cageid, cageid);
    let tp = sc_convert_addr_to_host(tp_arg, tp_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "clock_gettime", "Invalid Cage ID");
    }

    let ret = unsafe { syscall(SYS_clock_gettime, clockid, tp) as i32 };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "clock_gettime");
    }

    ret
}


pub fn dup_syscall(
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
        return syscall_error(Errno::EFAULT, "dup", "Invalid Cage ID");
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

pub fn dup2_syscall(
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
        return syscall_error(Errno::EFAULT, "dup2", "Invalid Cage ID");
    }

    match fdtables::translate_virtual_fd(cageid, old_vfd_arg) {
        Ok(old_vfd) => {
            let new_kernelfd = unsafe { libc::dup(old_vfd.underfd as i32) };
            // Map new kernel fd with provided kernel fd
            let _ret_kernelfd = unsafe { libc::dup2(old_vfd.underfd as i32, new_kernelfd) };
            let _ = fdtables::get_specific_virtual_fd(
                cageid,
                new_vfd_arg,
                old_vfd.fdkind,
                new_kernelfd as u64,
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


/// Reference to Linux: https://man7.org/linux/man-pages/man2/fchdir.2.html
///
/// Linux `fchdir()` syscall changes the current working directory of the calling process to the
/// directory referred to by the open file descriptor `fd`. Since we implement a file descriptor
/// management subsystem (called `fdtables`), we first translate the virtual file descriptor to the
/// corresponding kernel file descriptor before invoking the kernel's `libc::fchdir()` function.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment referring to a directory
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn fchdir_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "fchdir", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "fchdir", "Bad File Descriptor");
    }

    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fchdir", "Invalid Cage ID");
    }

    let ret = unsafe { libc::fchdir(kernel_fd) };
    if ret < 0 {
        return handle_errno(get_errno(), "fchdir");
    }

    // Update the cage's current working directory
    // We need to get the current working directory from the kernel to update the cage
    let mut cwd_buf = [0u8; PATH_MAX];
    let cwd_ptr = unsafe { libc::getcwd(cwd_buf.as_mut_ptr() as *mut i8, cwd_buf.len()) };
    if !cwd_ptr.is_null() {
        if let Some(cage) = get_cage(cageid) {
            let mut cwd = cage.cwd.write();
            *cwd = Arc::new(PathBuf::from(
                unsafe { std::ffi::CStr::from_ptr(cwd_ptr) }.to_string_lossy().as_ref()
            ));
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - iov_arg: pointer to an array of iovec structures describing the buffers (user's perspective)
///     - iovcnt_arg: number of iovec structures in the array
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn writev_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "writev", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "writev", "Bad File Descriptor");
    }

    let iovcnt = sc_convert_sysarg_to_i32(iovcnt_arg, iovcnt_cageid, cageid);
    if iovcnt < 0 {
        return syscall_error(Errno::EINVAL, "writev", "Invalid iovcnt");
    }

    let iov_ptr = sc_convert_buf(iov_arg, iov_cageid, cageid);
    if iov_ptr.is_null() {
        return syscall_error(Errno::EFAULT, "writev", "iovec is null");
    }

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "writev", "Invalid Cage ID");
    }

    let ret = unsafe {
        libc::writev(
            kernel_fd,
            iov_ptr as *const libc::iovec,
            iovcnt,
        ) as i32
    };
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - stat_arg: pointer to a stat structure where the file information will be stored (user's perspective)
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn fstat_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
    stat_arg: u64,
    stat_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Convert WASM virtual address to host address, then cast to StatData pointer
    let stat_ptr = sc_convert_buf(stat_arg, stat_cageid, cageid) as *mut StatData;
    if stat_ptr.is_null() {
        return syscall_error(Errno::EFAULT, "fstat", "Invalid stat buffer pointer");
    }
    
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "fstat", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "fstat", "Bad File Descriptor");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fstat", "Invalid Cage ID");
    }

    // 1) Call host fstat into a local host variable
    let mut host_stat: libc::stat = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::fstat(kernel_fd, &mut host_stat as *mut libc::stat) };
    if ret < 0 {
        return handle_errno(get_errno(), "fstat");
    }

    // 2) Validate guest buffer range and writability
    let needed_size = std::mem::size_of::<StatData>();
    if check_addr(stat_cageid, stat_arg, needed_size, PROT_WRITE).is_err() {
        return syscall_error(Errno::EFAULT, "fstat", "stat buffer not writable or too small");
    }

    // 3) Populate StatData directly
    unsafe {
        sc_convert_statdata(stat_ptr, &host_stat);
    }

    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/ftruncate.2.html
///
/// Linux `ftruncate()` syscall truncates the file referred to by the file descriptor `fd` to be at most
/// `length` bytes in size. Since we implement a file descriptor management subsystem (called `fdtables`),
/// we first translate the virtual file descriptor to the corresponding kernel file descriptor, then convert
/// the length argument from u64 to i64 type before invoking the kernel's `libc::ftruncate()` function.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - length_arg: the desired length in bytes for the file truncation
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn ftruncate_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "ftruncate", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "ftruncate", "Bad File Descriptor");
    }

    let length = sc_convert_sysarg_to_i64(length_arg, length_cageid, cageid);

    // Validate that length is not negative
    if length < 0 {
        return syscall_error(Errno::EINVAL, "ftruncate", "length cannot be negative");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "ftruncate", "Invalid Cage ID");
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - statfs_arg: pointer to a statfs structure where the filesystem information will be stored (user's perspective)
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn fstatfs_syscall(
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
    // Convert WASM virtual address to host address, then cast to FSData pointer
    let fsdata_ptr = sc_convert_buf(statfs_arg, statfs_cageid, cageid) as *mut FSData;
    if fsdata_ptr.is_null() {
        return syscall_error(Errno::EFAULT, "fstatfs", "Invalid statfs buffer pointer");
    }
    
    let kernel_fd = convert_fd_to_host(vfd_arg, vfd_cageid, cageid);
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "fstatfs", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "fstatfs", "Bad File Descriptor");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fstatfs", "Invalid Cage ID");
    }

    // 1) Call host fstatfs into a local host variable
    let mut host_statfs: libc::statfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::fstatfs(kernel_fd, &mut host_statfs as *mut libc::statfs) };
    if ret < 0 {
        return handle_errno(get_errno(), "fstatfs");
    }

    // 2) Validate guest buffer range and writability
    let needed_size = std::mem::size_of::<FSData>();
    if check_addr(statfs_cageid, statfs_arg, needed_size, PROT_WRITE).is_err() {
        return syscall_error(Errno::EFAULT, "fstatfs", "statfs buffer not writable or too small");
    }

    // 3) Populate FSData directly
    unsafe {
        sc_convert_fsdata(fsdata_ptr, &host_statfs);
    }

    0
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/getdents64.2.html
///
/// Linux `getdents()` syscall reads several directory entries from the directory referred to by the open file
/// descriptor `fd` into the buffer pointed to by `dirp`. Since we implement a file descriptor management
/// subsystem (called `fdtables`), we first translate the virtual file descriptor to the corresponding kernel
/// file descriptor, then call the kernel's `getdents64` syscall directly to retrieve directory entries.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment referring to a directory
///     - dirp_arg: pointer to a buffer where the directory entries will be stored (user's perspective)
///     - count_arg: size of the buffer pointed to by dirp
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn getdents_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "getdents", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "getdents", "Bad File Descriptor");
    }

    let dirp = sc_convert_buf(dirp_arg, dirp_cageid, cageid);
    if dirp.is_null() {
        return syscall_error(Errno::EFAULT, "getdents", "buffer is null");
    }
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getdents", "Invalid Cage ID");
    }

        let ret = unsafe {
        libc::syscall(libc::SYS_getdents64 as libc::c_long, kernel_fd, dirp, count) as i64
    };

    ret 
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/lseek.2.html
///
/// Linux `lseek()` syscall repositions the file offset of the open file description associated with the file
/// descriptor `fd` to the argument `offset` according to the directive `whence`. Since we implement a file
/// descriptor management subsystem (called `fdtables`), we first translate the virtual file descriptor to the
/// corresponding kernel file descriptor, then convert the offset and whence parameters from cage memory before
/// invoking the kernel's `libc::lseek()` function.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - offset_arg: the new offset according to the directive whence
///     - whence_arg: how to interpret the offset (SEEK_SET, SEEK_CUR, or SEEK_END)
///     - arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn lseek_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "lseek", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "lseek", "Bad File Descriptor");
    }

    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);
    let whence = sc_convert_sysarg_to_i32(whence_arg, whence_cageid, cageid);

    match whence {
        SEEK_SET | SEEK_CUR | SEEK_END => {},
        _ => return syscall_error(Errno::EINVAL, "lseek", "invalid whence parameter"),
    }

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "lseek", "Invalid Cage ID");
    }

    let ret = unsafe { libc::lseek(kernel_fd, offset, whence) };
    if ret < 0 {
        return handle_errno(get_errno(), "lseek");
    }

    // Check if the result is too large to fit in i32
    if ret > i32::MAX as i64 {
        return syscall_error(Errno::EOVERFLOW, "lseek", "result too large");
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - buf_arg: pointer to a buffer where the read data will be stored (user's perspective)
///     - count_arg: the maximum number of bytes to read from the file descriptor
///     - offset_arg: file offset at which the input/output operation takes place
///     - arg5, arg6: additional arguments which are expected to be unused
pub fn pread_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "pread", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "pread", "Bad File Descriptor");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    if buf.is_null() {
        return syscall_error(Errno::EFAULT, "pread", "Buffer is null");
    }

    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "pread", "Invalid Cage ID");
    }

    if count == 0 {
        return 0;
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - buf_arg: pointer to a buffer that stores the data to be written (user's perspective)
///     - count_arg: the maximum number of bytes to write to the file descriptor
///     - offset_arg: file offset at which the input/output operation takes place
///     - arg5, arg6: additional arguments which are expected to be unused
pub fn pwrite_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "pwrite", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "pwrite", "Bad File Descriptor");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    let offset = sc_convert_sysarg_to_i64(offset_arg, offset_cageid, cageid);

    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "pwrite", "Invalid Cage ID");
    }

    if count == 0 {
        return 0;
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - path_arg: pointer to a pathname naming the directory (user's perspective)
///     - path_cageid: cage identifier for the path argument
///     - arg2, arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// Return:
///     - return zero on success. On error, -1 is returned and errno is set to indicate the error.
pub fn chdir_syscall(
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
        return syscall_error(Errno::EFAULT, "chdir_syscall", "Invalid Cage ID");
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
        let mut cwd = cage.cwd.write();
        *cwd = Arc::new(PathBuf::from(path.to_string_lossy().as_ref()));
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/rmdir.2.html
///
/// Linux `rmdir()` syscall removes a directory, which must be empty. Since path seen by user is different
/// from actual path on host, we need to convert the path first. RawPOSIX doesn't have any other operations,
/// so all operations will be handled by host. RawPOSIX does error handling for this syscall.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - path_arg: pointer to a pathname naming the directory to be removed (user's perspective)
///     - path_cageid: cage identifier for the path argument
///     - arg2, arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// Return:
///     - return zero on success. On error, -1 is returned and errno is set to indicate the error.
pub fn rmdir_syscall(
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
        return syscall_error(Errno::EFAULT, "rmdir_syscall", "Invalid Cage ID");
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - path_arg: pointer to a pathname naming the file (user's perspective)
///     - path_cageid: cage identifier for the path argument
///     - mode_arg: the new file permissions (user's perspective)
///     - mode_cageid: cage identifier for the mode argument
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// Return:
///     - return zero on success. On error, -1 is returned and errno is set to indicate the error.
pub fn chmod_syscall(
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
        return syscall_error(Errno::EFAULT, "chmod_syscall", "Invalid Cage ID");
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
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - vfd_arg: the virtual file descriptor from the RawPOSIX environment
///     - mode_arg: the new file permissions to be applied to the file
///     - mode_cageid: cage identifier for the mode argument
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn fchmod_syscall(
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
    if kernel_fd == -(Errno::EINVAL as i32) {
        return syscall_error(Errno::EINVAL, "fchmod", "Invalid Cage ID");
    } else if kernel_fd == -(Errno::EBADF as i32) {
        return syscall_error(Errno::EBADF, "fchmod", "Bad File Descriptor");
    }

    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fchmod", "Invalid Cage ID");
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
/// Linux `getcwd()` syscall returns an absolute pathname that is the current working directory of the calling process.
/// The pathname is returned as a null-terminated string in the buffer pointed to by `buf`. Since path seen by user
/// is different from actual path on host, we need to convert the buffer pointer from cage memory to host memory
/// before invoking the kernel's `libc::getcwd()` function.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier
///     - buf_arg: pointer to a buffer where the current working directory path will be stored (user's perspective)
///     - size_arg: the size of the buffer in bytes
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
///
/// Return:
///     - On success, returns a pointer to the buffer containing the current working directory path
///     - On error, returns NULL and errno is set to indicate the error
pub fn getcwd_syscall(
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
    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    if buf.is_null() {
        return syscall_error(Errno::EFAULT, "getcwd", "Buffer is null");
    }

    let size = sc_convert_sysarg_to_usize(size_arg, size_cageid, cageid);
    if size == 0 {
        return syscall_error(Errno::EINVAL, "getcwd", "Size cannot be zero");
    }

    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "getcwd", "Invalid Cage ID");
    }

    let ret = unsafe { libc::getcwd(buf as *mut i8, size) };
    if ret.is_null() {
        let errno = get_errno();
        return handle_errno(errno, "getcwd");
    }

    // getcwd returns the buffer pointer on success, but we need to return the buffer address
    // in the user's perspective (cage memory address)
    buf_arg as i32
}

/// Truncate a file to a specified length
/// 
/// # Arguments
///     - cageid: current cage identifier
///     - path_arg: pointer to the pathname of the file to truncate
///     - path_cageid: cage identifier for the path argument
///     - length_arg: the new length to truncate the file to
///     - length_cageid: cage identifier for the length argument
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused
pub fn truncate_syscall(
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
         && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "truncate", "Invalid Cage ID");
    }

    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    if path.is_empty() {
        return syscall_error(Errno::EFAULT, "truncate", "Invalid path");
    }
    
    let length = sc_convert_sysarg_to_i64(length_arg, length_cageid, cageid);

    // Call libc truncate
    let ret = unsafe { libc::truncate(path.as_ptr() as *const i8, length) };
    
    if ret == -1 {
        let errno = get_errno();
        return handle_errno(errno, "truncate");
    }

    0
}
