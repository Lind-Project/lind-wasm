use parking_lot::RwLock;
use std::sync::atomic::{AtomicI32, AtomicU64};
use std::sync::Arc;
use typemap::type_conv::get_pipearray;
use typemap::fs_conv::*;

/// Helper function for close_syscall
/// 
/// Lind-WASM is running as same Linux-Process from host kernel perspective, so standard fds shouldn't
/// be closed in Lind-WASM execution, which preventing issues where other threads might reassign these
/// fds, causing unintended behavior or errors. 
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
    match fdtables::get_unused_virtual_fd(
        cageid,
        fs_const::FDKIND_KERNEL,
        kernel_fd as u64,
        should_cloexec,
        0,
    ) {
        Ok(virtual_fd) => virtual_fd as i32,
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
///     - virtual_fd: the virtual file descriptor from the RawPOSIX environment.
///     - buf_arg: pointer to a buffer where the read data will be stored (user's perspective).
///     - count_arg: the maximum number of bytes to read from the file descriptor.
pub fn read_syscall(
    cageid: u64,
    virtual_fd: u64,
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
    let kernel_fd = convert_fd_to_host(virtual_fd, vfd_cageid, cageid);
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
///     - virtual_fd: the virtual file descriptor from the RawPOSIX environment to be closed.
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused.
pub fn close_syscall(
    cageid: u64,
    virtual_fd: u64,
    vfd_cageid: u64, 
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

    match fdtables::close_virtualfd(cageid, virtual_fd) {
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
    // TODO: Check will perform in the below logic??
    println!("[mmap_syscall] selfcageid: {:?}, FD Cageid: {:?}", cageid, vfd_cageid);
    let mut addr = addr_arg as *mut u8;
    let mut len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let mut prot = sc_convert_sysarg_to_i32(prot_arg, prot_cageid, cageid);
    let mut flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let mut fildes = convert_fd_to_host(virtual_fd_arg, vfd_cageid, cageid);
    let mut off = sc_convert_sysarg_to_i64(off_arg, off_cageid, cageid);

    let cage = get_cage(cageid).unwrap();

    let mut maxprot = PROT_READ | PROT_WRITE;

    // only these four flags are allowed
    let allowed_flags =
        MAP_FIXED as i32 | MAP_SHARED as i32 | MAP_PRIVATE as i32 | MAP_ANONYMOUS as i32;
    if flags & !allowed_flags > 0 {
        // truncate flag to remove flags that are not allowed
        flags &= allowed_flags;
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
                    let flags = fcntl_syscall(cageid, fildes as u64, vfd_cageid, F_GETFL as u64, flags_cageid, 0, 0, 0, 0, 0, 0, 0, 0);
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
    println!("[mmap_inner] cageid: {:?}, vfd: {:?}", cageid, virtual_fd);
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
            return syscall_error(Errno::EINVAL, "mmap", "mmap failed with invalid flags") as usize;
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
    let addr = addr_arg as *mut u8;
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
    // Directly call libc::mmap to improve performance
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
    if result != sysaddr {
        panic!("MAP_FIXED not fixed");
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
    println!("[sbrk_syscall]");
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
