use libc::c_void;
use std::ffi::CStr;
use typemap::datatype_conversion::*;
use typemap::path_conversion::*;
use sysdefs::constants::err_const::{syscall_error, Errno, get_errno, handle_errno};
use sysdefs::constants::fs_const::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO, O_CLOEXEC, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE, PAGESHIFT, PAGESIZE};
use sysdefs::constants::lind_platform_const::{FDKIND_KERNEL, MAXFD, LIND_ROOT};
use sysdefs::constants::sys_const::{DEFAULT_UID, DEFAULT_GID};
use sysdefs::data::fs_struct::StatData;
use typemap::cage_helpers::*;
use cage::{round_up_page, get_cage, HEAP_ENTRY_INDEX, MemoryBackingType, VmmapOps};
use fdtables;

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
        return syscall_error(Errno::EFAULT, "open", "Invalide Cage ID");
    }

    // Get the kernel fd first
    let kernel_fd = unsafe { libc::open(path.as_ptr(), oflag, mode) };

    if kernel_fd < 0 {
        return handle_errno(get_errno(), "open");
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
        Ok(virtual_fd) => virtual_fd as i32,
        Err(_) => syscall_error(Errno::EMFILE, "open", "Too many files opened"),
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
    if !(sc_unusedarg(arg2, arg2_cageid)
         && sc_unusedarg(arg3, arg3_cageid)
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
        return syscall_error(Errno::EFAULT, "mkdir", "Invalide Cage ID");
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
    virtual_fd_arg: u64,
    vfd_cageid: u64,
    off_arg: u64,
    off_cageid: u64,
) -> i32 {
    let mut addr = sc_convert_to_u8_mut(addr_arg, addr_cageid, cageid);
    let mut len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let mut prot = sc_convert_sysarg_to_i32(prot_arg, prot_cageid, cageid);
    let mut flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let mut fildes = convert_fd_to_host(virtual_fd_arg, vfd_cageid, cageid);
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
    virtual_fd: i32,
    off: i64,
) -> usize {
    if virtual_fd != -1 {
        match fdtables::translate_virtual_fd(cageid, virtual_fd as u64) {
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
        return syscall_error(Errno::EFAULT, "munmap", "Invalide Cage ID");
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
        return syscall_error(Errno::EFAULT, "brk", "Invalide Cage ID");
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
        return syscall_error(Errno::EFAULT, "sbrk_syscall", "Invalide Cage ID");
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
/// virtual_fd: virtual file descriptor
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
pub fn _fcntl_helper(cageid: u64, virtual_fd: u64) -> Result<fdtables::FDTableEntry, Errno> {
    if virtual_fd > MAXFD as u64 {
        return Err(Errno::EBADF);
    }
    // Get underlying kernel fd
    let wrappedvfd = fdtables::translate_virtual_fd(cageid, virtual_fd);
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
/// virtual_fd: virtual file descriptor
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
    virtual_fd: u64,
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
        return syscall_error(Errno::EFAULT, "fcntl_syscall", "Invalide Cage ID");
    }

    match (cmd, arg) {
        // Duplicate the file descriptor `virtual_fd` using the lowest-numbered
        // available file descriptor greater than or equal to `arg`. The operation here
        // is quite similar to `dup_syscall`, for specific operation explanation, see
        // comments on `dup_syscall`.
        (F_DUPFD, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
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
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
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
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            return vfd.should_cloexec as i32;
        }
        // Set the file descriptor flags to the value specified by arg.
        (F_SETFD, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
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
            match fdtables::set_cloexec(cageid, virtual_fd as u64, cloexec_flag) {
                Ok(_) => return 0,
                Err(_e) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // todo: F_GETOWN and F_SETOWN commands are not implemented yet
        (F_GETOWN, ..) => DEFAULT_GID as i32,
        (F_SETOWN, arg) if arg >= 0 => 0,
        _ => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
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

//------------------------------------LINK SYSCALL------------------------------------
/*
 *  `link` creates a hard link to an existing file.
 *  Reference: https://man7.org/linux/man-pages/man2/link.2.html
 *
 *  ## Arguments:
 *   - `oldpath`: Path to the existing file.
 *   - `newpath`: Path where the hard link will be created.
 *
 *  ## Implementation Details:
 *   - Both paths are converted from the RawPOSIX perspective to the host kernel perspective
 *     using `sc_convert_path_to_host`, which handles the LIND_ROOT prefixing and path normalization.
 *   - The underlying libc::link() is called with both converted paths.
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn link_syscall(
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
        return syscall_error(Errno::EFAULT, "link", "Invalid Cage ID");
    }

    let ret = unsafe { libc::link(oldpath.as_ptr(), newpath.as_ptr()) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "link");
    }
    ret
}

//------------------------------------XSTAT SYSCALL------------------------------------
/*
 *  `xstat` retrieves file status information (versioned stat interface).
 *  Reference: https://man7.org/linux/man-pages/man2/stat.2.html
 *
 *  ## Arguments:
 *   - `vers`: Version parameter for stat structure compatibility.
 *   - `pathname`: Path to the file to get status information for.
 *   - `statbuf`: Buffer to store the file status information.
 *
 *  ## Implementation Details:
 *   - The path is converted from the RawPOSIX perspective to the host kernel perspective
 *     using `sc_convert_path_to_host`, which handles the LIND_ROOT prefixing and path normalization.
 *   - The statbuf buffer is converted from WASM address to host address using `sc_convert_addr_to_host`.
 *   - The underlying libc::stat() is called and results are copied to the user buffer.
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn stat_syscall(
    cageid: u64,
    vers_arg: u64,
    vers_cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    statbuf_arg: u64,
    statbuf_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let _vers = sc_convert_sysarg_to_i32(vers_arg, vers_cageid, cageid);
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "xstat", "Invalid Cage ID");
    }

    // Declare statbuf by ourselves
    let mut libc_statbuf: libc::stat = unsafe { std::mem::zeroed() };
    let libcret = unsafe { libc::stat(path.as_ptr(), &mut libc_statbuf) };

    if libcret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "xstat");
    }

    // Convert libc stat to StatData and copy to user buffer
    let statbuf_addr = sc_convert_uaddr_to_host(statbuf_arg, statbuf_cageid, cageid) as *mut StatData;
    let statdata = StatData {
        st_dev: libc_statbuf.st_dev,
        st_ino: libc_statbuf.st_ino as usize,
        st_mode: libc_statbuf.st_mode,
        st_nlink: libc_statbuf.st_nlink as u32,
        st_uid: libc_statbuf.st_uid,
        st_gid: libc_statbuf.st_gid,
        st_rdev: libc_statbuf.st_rdev,
        st_size: libc_statbuf.st_size as usize,
        st_blksize: libc_statbuf.st_blksize as i32,
        st_blocks: libc_statbuf.st_blocks as u32,
        st_atim: (libc_statbuf.st_atime as u64, libc_statbuf.st_atime_nsec as u64),
        st_mtim: (libc_statbuf.st_mtime as u64, libc_statbuf.st_mtime_nsec as u64),
        st_ctim: (libc_statbuf.st_ctime as u64, libc_statbuf.st_ctime_nsec as u64),
    };
    
    unsafe {
        std::ptr::copy_nonoverlapping(&statdata as *const StatData, statbuf_addr, 1);
    }

    libcret
}

//------------------------------------FSYNC SYSCALL------------------------------------
/*
 *  `fsync` synchronizes a file's in-core state with storage device.
 *  Reference: https://man7.org/linux/man-pages/man2/fsync.2.html
 *
 *  ## Arguments:
 *   - `fd`: File descriptor to synchronize.
 *
 *  ## Implementation Details:
 *   - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
 *   - This ensures proper translation between RawPOSIX virtual fds and host kernel fds.
 *   - The underlying libc::fsync() is called, which synchronizes both file data and metadata.
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn fsync_syscall(
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
        return syscall_error(Errno::EFAULT, "fsync", "Invalid Cage ID");
    }

    let kernel_fd = convert_fd_to_host(virtual_fd as u64, fd_cageid, cageid);
    if kernel_fd == -1 {
        return syscall_error(Errno::EFAULT, "fsync", "Invalid Cage ID");
    } else if kernel_fd == -9 {
        return syscall_error(Errno::EBADF, "fsync", "Bad File Descriptor");
    }

    let ret = unsafe { libc::fsync(kernel_fd) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "fsync");
    }
    return ret;
}

//------------------------------------FDATASYNC SYSCALL------------------------------------
/*
 *  `fdatasync` synchronizes a file's data to storage device (but not metadata).
 *  Reference: https://man7.org/linux/man-pages/man2/fdatasync.2.html
 *
 *  ## Arguments:
 *   - `fd`: File descriptor to synchronize.
 *
 *  ## Implementation Details:
 *   - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
 *   - This ensures proper translation between RawPOSIX virtual fds and host kernel fds.
 *   - The underlying libc::fdatasync() is called, which synchronizes only file data (not metadata
 *     like timestamps), making it potentially faster than fsync().
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn fdatasync_syscall(
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
        return syscall_error(Errno::EFAULT, "fdatasync", "Invalid Cage ID");
    }

    let kernel_fd = convert_fd_to_host(virtual_fd as u64, fd_cageid, cageid);
    if kernel_fd == -1 {
        return syscall_error(Errno::EFAULT, "fdatasync", "Invalid Cage ID");
    } else if kernel_fd == -9 {
        return syscall_error(Errno::EBADF, "fdatasync", "Bad File Descriptor");
    }

    let ret = unsafe { libc::fdatasync(kernel_fd) };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "fdatasync");
    }
    return ret;
}

//------------------------------------SYNC_FILE_RANGE SYSCALL------------------------------------
/*
 *  `sync_file_range` synchronizes a specific range of bytes in a file to storage device.
 *  Reference: https://man7.org/linux/man-pages/man2/sync_file_range.2.html
 *
 *  ## Arguments:
 *   - `fd`: File descriptor to synchronize.
 *   - `offset`: Starting byte offset for the range to sync.
 *   - `nbytes`: Number of bytes to synchronize.
 *   - `flags`: Flags controlling the synchronization behavior.
 *
 *  ## Implementation Details:
 *   - The virtual file descriptor is converted to a kernel file descriptor using `convert_fd_to_host`.
 *   - This ensures proper translation between RawPOSIX virtual fds and host kernel fds.
 *   - The underlying libc::sync_file_range() is called with the specified byte range and flags.
 *   - This is more efficient than fsync() for large files when only a specific range needs syncing.
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn sync_file_range_syscall(
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
    if !(sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "sync_file_range", "Invalid Cage ID");
    }

    let kernel_fd = convert_fd_to_host(virtual_fd as u64, fd_cageid, cageid);
    if kernel_fd == -1 {
        return syscall_error(Errno::EFAULT, "sync_file_range", "Invalid Cage ID");
    } else if kernel_fd == -9 {
        return syscall_error(Errno::EBADF, "sync", "Bad File Descriptor");
    }

    let ret = unsafe {
        libc::sync_file_range(kernel_fd, offset, nbytes, flags)
    };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "sync_file_range");
    }
    ret
}

//------------------------------------READLINKAT SYSCALL------------------------------------
/*
 *  `readlinkat` reads the value of a symbolic link.
 *  Reference: https://man7.org/linux/man-pages/man2/readlinkat.2.html
 *
 *  ## Arguments:
 *   - `virtual_fd`: File descriptor for directory (or AT_FDCWD for current working directory).
 *   - `path`: Path to the symbolic link to read.
 *   - `buf`: Buffer to store the link target.
 *   - `buflen`: Size of the buffer.
 *
 *  ## Implementation Details:
 *   - Handles both AT_FDCWD and explicit directory file descriptors.
 *   - Converts virtual file descriptor to kernel file descriptor using `convert_fd_to_host`.
 *   - Converts paths using `convpath` and `normpath` for proper path handling.
 *   - Adds LIND_ROOT prefix to create full kernel path.
 *   - Removes LIND_ROOT prefix from result before copying to user buffer.
 *
 *  ## Return Value:
 *   - Number of bytes placed in `buf` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn readlinkat_syscall(
    cageid: u64,
    virtual_fd_arg: u64,
    virtual_fd_cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    buflen_arg: u64,
    buflen_cageid: u64,
    arg8: u64,
    arg9: u64,
    arg10: u64,
    arg11: u64,
) -> i32 {
    let virtual_fd = sc_convert_sysarg_to_i32(virtual_fd_arg, virtual_fd_cageid, cageid);
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    let buf = sc_convert_uaddr_to_host(buf_arg, buf_cageid, cageid) as *mut u8;
    let buflen = sc_convert_sysarg_to_usize(buflen_arg, buflen_cageid, cageid);

    let mut libcret;
    let mut path = path.to_string_lossy().to_string();
    let libc_buflen = buflen + LIND_ROOT.len();
    let mut libc_buf = vec![0u8; libc_buflen];
    if virtual_fd == libc::AT_FDCWD {
        // Check if the fd is AT_FDCWD
        let cage_arc = get_cage(cageid).unwrap();
        let cwd_container = cage_arc.cwd.read();
        path = format!("{}/{}", cwd_container.to_str().unwrap(), path);
        // Convert the path from relative path (lind-wasm perspective) to real kernel path (host kernel
        // perspective)
        let relpath = normpath(convpath(&path), cageid);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = std::ffi::CString::new(full_path).unwrap();

        libcret = unsafe {
            libc::readlink(
                c_path.as_ptr(),
                libc_buf.as_mut_ptr() as *mut i8,
                libc_buflen,
            )
        };
    } else {
        // Convert the virtual fd into real kernel fd and handle the error case
        let kernel_fd = convert_fd_to_host(virtual_fd as u64, virtual_fd_cageid, cageid);
        if kernel_fd == -1 {
            return syscall_error(Errno::EFAULT, "readlinkat", "Invalid Cage ID");
        } else if kernel_fd == -9 {
            return syscall_error(Errno::EBADF, "readlinkat", "Bad File Descriptor");
        }
        // Convert the path from relative path (lind-wasm perspective) to real kernel path (host kernel
        // perspective)
        let relpath = normpath(convpath(&path), cageid);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = std::ffi::CString::new(full_path).unwrap();

        libcret = unsafe {
            libc::readlinkat(
                kernel_fd,
                c_path.as_ptr(),
                libc_buf.as_mut_ptr() as *mut i8,
                libc_buflen,
            )
        };
    }

    if libcret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "readlinkat");
    }

    // Convert the result from readlink to a Rust string
    let libcbuf_str = unsafe { CStr::from_ptr(libc_buf.as_ptr() as *const i8) }
        .to_str()
        .unwrap();

    // Adjust the result to remove LIND_ROOT prefix if present
    let new_root = format!("{}/", LIND_ROOT);
    let final_result = libcbuf_str
        .strip_prefix(&new_root)
        .unwrap_or(libcbuf_str);

    // Check the length and copy the appropriate amount of data to buf
    let bytes_to_copy = std::cmp::min(buflen, final_result.len());
    unsafe {
        std::ptr::copy_nonoverlapping(final_result.as_ptr(), buf as *mut u8, bytes_to_copy);
    }

    bytes_to_copy as i32
}

//------------------------------------RENAME SYSCALL------------------------------------
/*
 *  `rename` renames a file, moving it between directories if required.
 *  Reference: https://man7.org/linux/man-pages/man2/rename.2.html
 *
 *  ## Arguments:
 *   - `oldpath`: Current path of the file.
 *   - `newpath`: New path for the file.
 *
 *  ## Implementation Details:
 *   - Both paths are converted from the RawPOSIX perspective to the host kernel perspective
 *     using `sc_convert_path_to_host`, which handles the LIND_ROOT prefixing and path normalization.
 *   - The underlying libc::rename() is called with both converted paths.
 *   - This can move files across directories and rename them simultaneously.
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn rename_syscall(
    cageid: u64,
    oldpath_arg: u64,
    oldpath_cageid: u64,
    newpath_arg: u64,
    newpath_cageid: u64,
    arg5: u64,
    arg6: u64,
    arg7: u64,
    arg8: u64,
    arg9: u64,
    arg10: u64,
    arg11: u64,
    arg12: u64,
) -> i32 {
    let oldpath = sc_convert_path_to_host(oldpath_arg, oldpath_cageid, cageid);
    let newpath = sc_convert_path_to_host(newpath_arg, newpath_cageid, cageid);

    let old_c_path = std::ffi::CString::new(oldpath.to_str().unwrap()).unwrap();
    let new_c_path = std::ffi::CString::new(newpath.to_str().unwrap()).unwrap();

    let result = unsafe { libc::rename(old_c_path.as_ptr(), new_c_path.as_ptr()) };

    if result < 0 {
        let errno = get_errno();
        return handle_errno(errno, "rename");
    }

    result
}

//------------------------------------ACCESS SYSCALL------------------------------------
/*
 *  `access` checks whether the calling process can access the file pathname.
 *  Reference: https://man7.org/linux/man-pages/man2/access.2.html
 *
 *  ## Arguments:
 *   - `pathname`: Path to the file to check.
 *   - `mode`: Specifies the accessibility check(s) to be performed (F_OK, R_OK, W_OK, X_OK).
 *
 *  ## Implementation Details:
 *   - The pathname is converted from the RawPOSIX perspective to the host kernel perspective
 *     using `sc_convert_path_to_host`, which handles the LIND_ROOT prefixing and path normalization.
 *   - The underlying libc::access() is called with the converted path and mode.
 *
 *  ## Return Value:
 *   - `0` on success (all requested permissions are granted).
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn access_syscall(
    cageid: u64,
    pathname_arg: u64,
    pathname_cageid: u64,
    mode_arg: u64,
    mode_cageid: u64,
    arg5: u64,
    arg6: u64,
    arg7: u64,
    arg8: u64,
    arg9: u64,
    arg10: u64,
    arg11: u64,
    arg12: u64,
) -> i32 {
    let pathname = sc_convert_path_to_host(pathname_arg, pathname_cageid, cageid);
    let mode = sc_convert_sysarg_to_i32(mode_arg, mode_cageid, cageid);

    let c_path = std::ffi::CString::new(pathname.to_str().unwrap()).unwrap();

    let result = unsafe { libc::access(c_path.as_ptr(), mode) };

    if result < 0 {
        let errno = get_errno();
        return handle_errno(errno, "access");
    }

    result
}

//------------------------------------UNLINKAT SYSCALL------------------------------------
/*
 *  `unlinkat` deletes a file or directory relative to a directory file descriptor.
 *  Reference: https://man7.org/linux/man-pages/man2/unlinkat.2.html
 *
 *  ## Arguments:
 *   - `dirfd`: Directory file descriptor (or AT_FDCWD for current working directory).
 *   - `pathname`: Path of the file/directory to remove.
 *   - `flags`: Control flags (e.g., AT_REMOVEDIR for directories).
 *
 *  ## Implementation Details:
 *   - Handles both AT_FDCWD and explicit directory file descriptors.
 *   - Converts virtual file descriptor to kernel file descriptor using `convert_fd_to_host`.
 *   - Converts paths using `sc_convert_path_to_host` for proper path handling.
 *   - Supports AT_REMOVEDIR flag for removing directories.
 *
 *  ## Return Value:
 *   - `0` on success.
 *   - `-1` on failure, with `errno` set appropriately.
 */
pub fn unlinkat_syscall(
    cageid: u64,
    dirfd_arg: u64,
    dirfd_cageid: u64,
    pathname_arg: u64,
    pathname_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    _arg7: u64,
    _arg8: u64,
    _arg9: u64,
    _arg10: u64,
    _arg11: u64,
    _arg12: u64,
) -> i32 {
    let virtual_fd = sc_convert_sysarg_to_i32(dirfd_arg, dirfd_cageid, cageid);
    let pathname = sc_convert_path_to_host(pathname_arg, pathname_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    let result = if virtual_fd == libc::AT_FDCWD {
        // Case 1: AT_FDCWD - path is already converted by sc_convert_path_to_host
        unsafe {
            libc::unlinkat(
                libc::AT_FDCWD,
                pathname.as_ptr(),
                flags,
            )
        }
    } else {
        // Case 2: Specific directory fd
        let kernel_fd = convert_fd_to_host(virtual_fd as u64, dirfd_cageid, cageid);
        if kernel_fd == -1 {
            return syscall_error(Errno::EFAULT, "unlinkat", "Invalid Cage ID");
        } else if kernel_fd == -9 {
            return syscall_error(Errno::EBADF, "unlinkat", "Bad File Descriptor");
        }

        unsafe {
            libc::unlinkat(
                kernel_fd,
                pathname.as_ptr(),
                flags,
            )
        }
    };

    if result < 0 {
        let errno = get_errno();
        return handle_errno(errno, "unlinkat");
    }

    result
}
