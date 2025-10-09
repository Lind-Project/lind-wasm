use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{
    F_GETFL, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PAGESHIFT, PAGESIZE, PROT_EXEC,
    PROT_NONE, PROT_READ, PROT_WRITE,
};

use crate::interface::cagetable_getref;
use crate::safeposix::cage::Cage;
use crate::safeposix::shm::ShmMetadata;
use crate::safeposix::shm::{get_shm_length, SHM_METADATA};
use crate::safeposix::vmmap::{MemoryBackingType, Vmmap, VmmapOps};
use std::result::Result;
use std::sync::Arc;
use sysdefs::constants::SHM_RDONLY;

// heap is placed at the very top of the memory
pub const HEAP_ENTRY_INDEX: u32 = 0;

/// Round up the address length to be multiple of pages
///
/// # Arguments
/// * `length` - length of the address
///
/// # Returns
/// * `u64` - rounded up length
pub fn round_up_page(length: u64) -> u64 {
    if length % PAGESIZE as u64 == 0 {
        length
    } else {
        ((length / PAGESIZE as u64) + 1) * PAGESIZE as u64
    }
}

/// Copies the memory regions from parent to child based on the provided `vmmap` memory layout.
///
/// This function is designed to replicate the parent's memory space into the child immediately after
/// a `fork_syscall` in Wasmtime. It assumes that the parent and child share the same `vmmap` structure,
/// a valid assumption in this context.
///
/// The copying behavior varies based on the type of memory region:
/// 1. **PROT_NONE regions**:
///    - No action is taken, as memory regions are already configured with `PROT_NONE` by default.
/// 2. **Shared memory regions**:
///    - The function uses the `mremap` syscall to replicate shared memory efficiently. Refer to `man 2 mremap` for details.
/// 3. **Private memory regions**:
///    - The function uses `std::ptr::copy_nonoverlapping` to copy the memory contents directly.
///    - **TODO**: Investigate whether using `writev` could improve performance for this case.
///
/// # Arguments
/// * `parent_vmmap` - vmmap struct of parent
/// * `child_vmmap` - vmmap struct of child
pub fn fork_vmmap(parent_vmmap: &Vmmap, child_vmmap: &Vmmap) {
    let parent_base = parent_vmmap.base_address.unwrap();
    let child_base = child_vmmap.base_address.unwrap();

    // iterate through each vmmap entry
    for (_interval, entry) in parent_vmmap.entries.iter() {
        // translate page number to user address
        let addr_st = (entry.page_num << PAGESHIFT) as u32;
        let addr_len = (entry.npages << PAGESHIFT) as usize;

        // translate user address to system address
        let parent_st = parent_vmmap.user_to_sys(addr_st);
        let child_st = child_vmmap.user_to_sys(addr_st);
        if entry.flags & (MAP_SHARED as i32) > 0 {
            // for shared memory, we are using mremap to fork shared memory
            // See "man 2 mremap" for description of what MREMAP_MAYMOVE does with old_size=0
            // when old_address points to a shared mapping
            let result = unsafe {
                libc::mremap(
                    parent_st as *mut libc::c_void,
                    0,
                    addr_len,
                    libc::MREMAP_MAYMOVE | libc::MREMAP_FIXED,
                    child_st as *mut libc::c_void,
                )
            };
        } else {
            unsafe {
                // temporarily enable write on child's memory region to write parent data
                libc::mprotect(
                    child_st as *mut libc::c_void,
                    addr_len,
                    PROT_READ | PROT_WRITE,
                );

                // write parent data
                // TODO: replace copy_nonoverlapping with writev for potential performance boost
                std::ptr::copy_nonoverlapping(
                    parent_st as *const u8,
                    child_st as *mut u8,
                    addr_len,
                );

                // revert child's memory region prot
                libc::mprotect(child_st as *mut libc::c_void, addr_len, entry.prot)
            };
        }
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
pub fn munmap_handler(cageid: u64, addr: *mut u8, len: usize) -> i32 {
    let cage = cagetable_getref(cageid);

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64) as usize;
    if rounded_addr != addr as usize {
        return syscall_error(Errno::EINVAL, "mmap", "address it not aligned");
    }

    let vmmap = cage.vmmap.read();
    let sysaddr = vmmap.user_to_sys(rounded_addr as u32);
    drop(vmmap);

    let rounded_length = round_up_page(len as u64) as usize;

    // we are replacing munmap with mmap because we do not want to really deallocate the memory region
    // we just want to set the prot of the memory region back to PROT_NONE
    let result = cage.mmap_syscall(
        sysaddr as *mut u8,
        rounded_length,
        PROT_NONE,
        (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32,
        -1,
        0,
    );
    if result != sysaddr {
        panic!("MAP_FIXED not fixed");
    }

    let mut vmmap = cage.vmmap.write();

    let _ = vmmap.remove_entry(rounded_addr as u32 >> PAGESHIFT, len as u32 >> PAGESHIFT);

    0
}

/// Handles the shmat syscall by mapping shared memory segments into the cage's address space.
/// This function manages the attachment of shared memory segments by updating the cage's vmmap
/// and handling the raw shmat syscall.
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
pub fn shmat_handler(cageid: u64, addr: *mut u8, mut prot: i32, shmflag: i32, shmid: i32) -> u32 {
    // println!("shmat start");
    // Get the cage reference.
    let cage = cagetable_getref(cageid);

    // If SHM_RDONLY is set in shmflag, then use read-only protection,
    // otherwise default to read–write.
    prot = if shmflag & SHM_RDONLY != 0 {
        PROT_READ
    } else {
        PROT_READ | PROT_WRITE
    };
    let len = match get_shm_length(shmid) {
        Some(l) => l,
        None => return syscall_error(Errno::EINVAL, "shmat", "invalid shmid") as u32,
    };

    // Check that the provided address is page aligned.
    let rounded_addr = round_up_page(addr as u64);
    if rounded_addr != addr as u64 {
        return syscall_error(Errno::EINVAL, "shmat", "address is not aligned") as u32;
    }

    // Round up the length to a multiple of the page size.
    let rounded_length = round_up_page(len as u64);

    // Initialize the user address from the provided address pointer.
    // If addr is null (0), we need to allocate memory space from the virtual memory map (vmmap).
    // println!("before vmmap");
    let mut useraddr = addr as u32;
    let mut vmmap = cage.vmmap.write();
    let result;
    // println!("after vmmap");
    if useraddr == 0 {
        // Allocate a suitable space in the virtual memory map for the shared memory segment
        // based on the rounded length of the segment.
        result = vmmap.find_map_space((rounded_length >> PAGESHIFT) as u32, 1);
    } else {
        // Use the user-specified address as a hint to find an appropriate memory address
        // for the shared memory segment.
        result = vmmap.find_map_space_with_hint(rounded_length as u32 >> PAGESHIFT, 1, addr as u32 >> PAGESHIFT);
    }
    if result.is_none() {
        // If no suitable memory space is found, return an error indicating insufficient memory.
        return syscall_error(Errno::ENOMEM, "shmat", "no memory") as u32;
    }
    let space = result.unwrap();
    // Update the user address to the start of the allocated memory space.
    useraddr = (space.start() << PAGESHIFT) as u32;
    // println!("after vmmap 0.5");
    drop(vmmap);

    // Convert the user address into a system address.
    // Read the virtual memory map to access the user address space.
    let vmmap = cage.vmmap.read();
    // Convert the user address to the corresponding system address for the shared memory segment.
    let sysaddr = vmmap.user_to_sys(useraddr);
    // Release the lock on the virtual memory map as we no longer need it.
    drop(vmmap);
    // println!("after vmmap 1");

    // println!("sysaddr: {:?}", sysaddr);
    // Call the raw shmat syscall to attach the shared memory segment.
    let result = cage.shmat_syscall(shmid, sysaddr as *mut u8, shmflag);
    // println!("raw result: {}", result);
    let vmmap = cage.vmmap.read();
    let result = vmmap.sys_to_user(result);
    drop(vmmap);

    // If the syscall succeeded, update the vmmap entry.
    if result as i32 >= 0 {
        // println!("useraddr: {}, result: {}", useraddr, result);
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
        let add_result = vmmap.add_entry_with_overwrite(
            useraddr >> PAGESHIFT,
            (rounded_length >> PAGESHIFT) as u32,
            prot,
            maxprot,
            0, // No flags for shared memory mapping
            backing,
            0, // Offset is not applicable for shared memory
            len as i64,
            cageid,
        );

        // Check if adding the entry was successful.
        if add_result.is_err() {
            return syscall_error(Errno::ENOMEM, "shmat", "failed to add vmmap entry") as u32;
        }
    } else {
        // If the syscall failed, propagate the error.
        return result as u32;
    }

    useraddr as u32
}

/// Handler of the `shmdt_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `shmdt_syscall` by updating the `vmmap` entries and managing
/// the shared memory detachment operation. It performs address validation, converts user
/// addresses to system addresses, and updates the virtual memory mappings accordingly.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that calls the `shmdt`
/// * `addr` - Starting address of the shared memory region to detach
///
/// # Returns
/// * `i32` - 0 for success and negative errno for failure
pub fn shmdt_handler(cageid: u64, addr: *mut u8) -> i32 {
    // Retrieve the cage reference.
    let cage = cagetable_getref(cageid);

    // Check that the provided address is aligned on a page boundary.
    let rounded_addr = round_up_page(addr as u64) as usize;
    if rounded_addr != addr as usize {
        return syscall_error(Errno::EINVAL, "shmdt", "address is not aligned");
    }

    // Convert the user address into a system address using the vmmap.
    let vmmap = cage.vmmap.read();
    let sysaddr = vmmap.user_to_sys(rounded_addr as u32);
    drop(vmmap);

    // Call shmdt_syscall which returns length of the detached segment
    let length = cage.shmdt_syscall(sysaddr as *mut u8);
    if length < 0 {
        return length;
    }

    // Remove the mapping from the vmmap.
    // This call removes the range starting at the page-aligned user address,
    // for the number of pages that cover the shared memory region.
    let mut vmmap = cage.vmmap.write();
    vmmap.remove_entry(
        rounded_addr as u32 >> PAGESHIFT,
        (length as u32) >> PAGESHIFT,
    );

    0
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
pub fn mmap_handler(
    cageid: u64,
    addr: *mut u8,
    len: usize,
    mut prot: i32,
    mut flags: i32,
    mut fildes: i32,
    off: i64,
) -> u32 {
    let cage = cagetable_getref(cageid);

    let mut maxprot = PROT_READ | PROT_WRITE;

    // only these four flags are allowed
    let allowed_flags =
        MAP_FIXED as i32 | MAP_SHARED as i32 | MAP_PRIVATE as i32 | MAP_ANONYMOUS as i32;
    if flags & !allowed_flags > 0 {
        // truncate flag to remove flags that are not allowed
        flags &= allowed_flags;
    }

    if prot & PROT_EXEC > 0 {
        return syscall_error(Errno::EINVAL, "mmap", "PROT_EXEC is not allowed") as u32;
    }

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64);
    if rounded_addr != addr as u64 {
        return syscall_error(Errno::EINVAL, "mmap", "address it not aligned") as u32;
    }

    // offset should be non-negative and multiple of pages
    if off < 0 {
        return syscall_error(Errno::EINVAL, "mmap", "offset cannot be negative") as u32;
    }
    let rounded_off = round_up_page(off as u64);
    if rounded_off != off as u64 {
        return syscall_error(Errno::EINVAL, "mmap", "offset it not aligned") as u32;
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
                vmmap.find_map_space_with_hint(rounded_length as u32 >> PAGESHIFT, 1, addr as u32 >> PAGESHIFT);
        }

        // did not find desired memory region
        if result.is_none() {
            return syscall_error(Errno::ENOMEM, "mmap", "no memory") as u32;
        }

        let space = result.unwrap();
        useraddr = (space.start() << PAGESHIFT) as u32;
    }

    flags |= MAP_FIXED as i32;

    // either MAP_PRIVATE or MAP_SHARED should be set, but not both
    if (flags & MAP_PRIVATE as i32 == 0) == (flags & MAP_SHARED as i32 == 0) {
        return syscall_error(Errno::EINVAL, "mmap", "invalid flags") as u32;
    }

    let vmmap = cage.vmmap.read();

    let sysaddr = vmmap.user_to_sys(useraddr);

    drop(vmmap);

    if rounded_length > 0 {
        if flags & MAP_ANONYMOUS as i32 > 0 {
            fildes = -1;
        }

        let result = cage.mmap_syscall(
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
                    let flags = cage.fcntl_syscall(fildes, F_GETFL, 0);
                    if flags < 0 {
                        return syscall_error(Errno::EINVAL, "mmap", "invalid file descriptor")
                            as u32;
                    }
                    maxprot &= flags;
                    MemoryBackingType::FileDescriptor(fildes as u64)
                }
            };

            // update vmmap entry
            let add_result = vmmap.add_entry_with_overwrite(
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

            // Check if adding the entry was successful.
            if add_result.is_err() {
                return syscall_error(Errno::ENOMEM, "mmap", "failed to add vmmap entry") as u32;
            }
        }
    }

    useraddr as u32
}

/// Handles the `mprotect_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `mprotect_syscall` by updating the `vmmap` entries and performing
/// the necessary protection changes. The handling logic is as follows:
/// 1. Validate protection flags - specifically disallow `PROT_EXEC`
/// 2. Verify address alignment and round to page boundaries
/// 3. Check if the memory region is mapped in vmmap
/// 4. Perform the actual mprotect syscall
/// 5. Update the protection flags in vmmap entries, splitting entries if necessary
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `mprotect` syscall
/// * `addr` - Starting address of the region to change protection
/// * `len` - Length of the region to change protection
/// * `prot` - New protection flags (e.g., `PROT_READ`, `PROT_WRITE`)
///
/// # Returns
/// * `i32` - Returns 0 on success, -1 on failure
pub fn mprotect_handler(cageid: u64, addr: *mut u8, len: usize, prot: i32) -> i32 {
    let cage = cagetable_getref(cageid);

    // PROT_EXEC is not allowed in WASM
    // TODO: Remove this panic when we support PROT_EXEC for real user code
    if prot & PROT_EXEC > 0 {
        // Log the attempt through syscall_error's verbose logging
        let _ = syscall_error(
            Errno::EINVAL,
            "mprotect",
            "PROT_EXEC attempt detected - this will panic in development",
        );
        // Panic during development for early detection of unsupported operations
        panic!("PROT_EXEC is not currently supported in WASM");
    }

    // Validate length
    if len == 0 {
        return syscall_error(Errno::EINVAL, "mprotect", "length cannot be zero");
    }

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64);
    if rounded_addr != addr as u64 {
        return syscall_error(Errno::EINVAL, "mprotect", "address is not aligned");
    }

    // round up length to be multiple of pages
    let rounded_length = round_up_page(len as u64);

    let mut vmmap = cage.vmmap.write();

    // Convert to page numbers for vmmap checking
    let start_page = (addr as u32) >> PAGESHIFT;
    let npages = (rounded_length >> PAGESHIFT) as u32;

    // Check if the region is mapped
    if !vmmap.check_existing_mapping(start_page, npages, 0) {
        return syscall_error(Errno::ENOMEM, "mprotect", "Address range not mapped");
    }

    // Get system address for the actual mprotect call
    let sysaddr = vmmap.user_to_sys(addr as u32);

    drop(vmmap);

    // Perform mprotect through cage implementation
    let result = cage.mprotect_syscall(sysaddr as *mut u8, rounded_length as usize, prot);

    if result < 0 {
        return result;
    }

    // Update vmmap entries with new protection
    let mut vmmap = cage.vmmap.write();
    vmmap.update_protections(start_page, npages, prot);

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
pub fn sbrk_handler(cageid: u64, brk: i32) -> u32 {
    let cage = cagetable_getref(cageid);

    // get the heap entry
    let mut vmmap = cage.vmmap.read();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    // program break should always be the same as the heap entry end
    assert!(heap.npages == vmmap.program_break);

    // pass 0 to sbrk will just return the current brk
    if brk == 0 {
        return (PAGESIZE * heap.npages) as u32;
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

    if brk_handler(
        cageid,
        ((heap.npages as i32 + brk_page) << PAGESHIFT) as u32,
    ) < 0
    {
        return syscall_error(Errno::ENOMEM, "sbrk", "no memory") as u32;
    }

    // sbrk syscall should return previous brk address before increment
    (PAGESIZE * heap.npages) as u32
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
pub fn brk_handler(cageid: u64, brk: u32) -> i32 {
    let cage = cagetable_getref(cageid);

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
    let _ = vmmap.add_entry_with_overwrite(
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
        let ret = cage.mmap_syscall(
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
        let ret = cage.mmap_syscall(
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

// set the wasm linear memory base address to vmmap
pub fn init_vmmap_helper(cageid: u64, base_address: usize, program_break: Option<u32>) {
    let cage = cagetable_getref(cageid);
    let mut vmmap = cage.vmmap.write();
    vmmap.set_base_address(base_address);
    if program_break.is_some() {
        vmmap.set_program_break(program_break.unwrap());
    }
}

// clone the cage memory. Invoked by wasmtime after cage is forked
pub fn fork_vmmap_helper(parent_cageid: u64, child_cageid: u64) {
    let parent_cage = cagetable_getref(parent_cageid);
    let child_cage = cagetable_getref(child_cageid);
    let parent_vmmap = parent_cage.vmmap.read();
    let child_vmmap = child_cage.vmmap.read();

    fork_vmmap(&parent_vmmap, &child_vmmap);

    // update program break for child
    drop(child_vmmap);
    let mut child_vmmap = child_cage.vmmap.write();
    child_vmmap.set_program_break(parent_vmmap.program_break);
}

/// Validates and converts a virtual memory address to a physical address with protection checks
///
/// This function performs several critical memory management operations:
/// 1. Validates that the requested memory region is properly mapped
/// 2. Checks protection flags match the requested access
/// 3. Converts virtual addresses to physical addresses
///
/// # Arguments
/// * `cage` - Reference to the memory cage containing the virtual memory map
/// * `arg` - Virtual memory address to check and convert
/// * `length` - Length of the memory region being accessed
/// * `prot` - Protection flags to validate (read/write/execute)
///
/// # Returns
/// * `Ok(u64)` - Physical memory address if validation succeeds
/// * `Err(Errno)` - EFAULT if memory access would be invalid
///
/// # Memory Safety
/// This is a critical security function that prevents invalid memory accesses by:
/// - Ensuring addresses are properly aligned to pages
/// - Validating all pages in the region are mapped with correct permissions
/// - Preventing access outside of allocated memory regions
pub fn check_and_convert_addr_ext(
    cage: &Cage,
    arg: u64,
    length: usize,
    prot: i32,
) -> Result<u64, Errno> {
    // Get read lock on virtual memory map
    let mut vmmap = cage.vmmap.write();

    // Calculate page numbers for start and end of region
    let page_num = (arg >> PAGESHIFT) as u32; // Starting page number
    let end_page = ((arg + length as u64 + PAGESIZE as u64 - 1) >> PAGESHIFT) as u32; // Ending page number (rounded up)
    let npages = end_page - page_num; // Total number of pages spanned

    // Validate memory mapping and permissions
    if vmmap.check_addr_mapping(page_num, npages, prot).is_none() {
        return Err(Errno::EFAULT); // Return error if mapping invalid
    }

    // Convert to physical address by adding base address
    Ok(vmmap.base_address.unwrap() as u64 + arg)
}

/// This function translates a virtual memory address to a physical address by adding the base address of the vmmap to the argument.
///
/// # Arguments
/// * `cage` - Reference to the memory cage containing the virtual memory map
/// * `arg` - Virtual memory address to translate
///
/// # Returns
/// * `Ok(u64)` - Translated physical memory address. NULL if user address is NULL.
pub fn translate_vmmap_addr(cage: &Cage, arg: u64) -> Result<u64, Errno> {
    // do not convert NULL pointer
    if arg == 0 {
        return Ok(0);
    }
    // Get read lock on virtual memory map
    let vmmap = cage.vmmap.read();
    Ok(vmmap.base_address.unwrap() as u64 + arg)
}
