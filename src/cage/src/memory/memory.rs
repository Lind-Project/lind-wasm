//! VMMAP helper functions
//!
//! This file provides helper functions related to vmmap, including address alignment,
//! initializing vmmap, helper functions for handling vmmap during a fork syscall, and
//! address translation and validation related to vmmap
use crate::cage::{get_cage, Cage};
use crate::memory::VmmapOps;
use sysdefs::constants::err_const::{get_errno, Errno};
use sysdefs::constants::fs_const::{
    MAP_SHARED, MREMAP_FIXED, MREMAP_MAYMOVE, PAGESHIFT, PAGESIZE, PROT_NONE, PROT_READ, PROT_WRITE,
};
use sysdefs::{lind_debug_panic, lind_log};

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

/// Check if a return value from libc::mmap indicates an error
///
/// Valid mmap addresses are always page-aligned. This function uses page alignment
/// to detect errors, as libc::mmap returns -1 (cast to usize) on error, which is
/// not page-aligned. As a defensive measure, if an unaligned value is detected that
/// falls outside the expected errno range (-1 to -PAGESIZE), the function panics.
///
/// # Arguments
/// * `ret` - return value from libc::mmap cast to usize
///
/// # Returns
/// * `bool` - true if ret indicates an error, false if it's a valid address
pub fn is_mmap_error(ret: usize) -> bool {
    // Check if page-aligned first (normal case)
    if ret % PAGESIZE as usize == 0 {
        return false; // Not an error
    }

    // If not aligned, verify it's in the valid errno range
    // Valid errno values are -1 to -PAGESIZE, which when cast to usize are:
    // usize::MAX - PAGESIZE + 1 to usize::MAX
    let min_errno = usize::MAX - (PAGESIZE as usize) + 1;
    if ret >= min_errno {
        return true; // Valid error in errno range
    }

    // Unaligned but not in errno range - this should never happen
    lind_debug_panic!(
        "mmap returned unaligned address outside errno range: 0x{:x}",
        ret
    );
    true // treat as error in LogOnly/NoAction mode
}

pub fn get_base_address(cageid: u64) -> usize {
    let cage = get_cage(cageid).unwrap();
    let vmmap = cage.vmmap.read();
    vmmap.get_base_address()
}

/// Copies the memory regions from parent to child based on the provided `vmmap` memory layout.
///
/// This function is designed to replicate the parent's memory space into the child immediately after
/// a `fork_syscall` in Wasmtime. At the time of fork, the child's `vmmap` is created as an exact copy
/// of the parent's `vmmap`, ensuring both start with identical memory mappings. Subsequent changes
/// to either `vmmap` do not affect the other.
///
/// The copying behavior varies based on the type of memory region:
/// 1. **PROT_NONE regions**:
///    - No action is taken, as memory regions are already configured with `PROT_NONE` by default.
/// 2. **Shared memory regions**:
///    - The function uses the `mremap` syscall to replicate shared memory efficiently. Refer to `man 2 mremap` for details.
/// 3. **Private memory regions**:
///    - The function uses `process_vm_writev` to copy memory contents from the parent into
///      the child's address space.
///
/// # Arguments
/// * `parent_cageid` - cageid of parent
/// * `child_cageid` - caegid of child
pub fn fork_vmmap(parent_cageid: u64, child_cageid: u64) {
    eprintln!("[fork_vmmap] begin");
    // first retrieve corresponding vmmaps
    let parent_cage = get_cage(parent_cageid).unwrap();
    let child_cage = get_cage(child_cageid).unwrap();
    let parent_vmmap = parent_cage.vmmap.read();
    let child_vmmap = child_cage.vmmap.read();

    // iterate through each vmmap entry
    for (interval, entry) in parent_vmmap.entries.iter() {
        // PROT_NONE regions are already configured with PROT_NONE by default,
        // and reading from a host PROT_NONE page would cause a SIGSEGV
        if entry.prot == PROT_NONE {
            continue;
        }

        // translate page number to user address
        let addr_st = (entry.page_num << PAGESHIFT) as u32;
        let addr_len = (entry.npages << PAGESHIFT) as usize;

        // translate user address to system address
        let parent_st = parent_vmmap.user_to_sys(addr_st);
        let child_st = child_vmmap.user_to_sys(addr_st);

        // eprintln!(
        //     "[fork_vmmap] interval={:?}, user={:#x}, len={:#x}, \
        //     parent={:#x}, child={:#x}, prot={:#x}, flags={:#x}, backing={:?}",
        //     interval,
        //     addr_st,
        //     addr_len,
        //     parent_st,
        //     child_st,
        //     entry.prot,
        //     entry.flags,
        //     entry.backing,
        // );
        let hit = addr_st <= 0xffffe000
            && 0xffffe000 < addr_st.wrapping_add(addr_len as u32);

        if hit {
            eprintln!(
                "[fork_vmmap-hit-ffffe000] user=[{:#x},{:#x}) len={:#x} parent={:#x} child={:#x} prot={:#x} flags={:#x} backing={:?}",
                addr_st,
                addr_st.wrapping_add(addr_len as u32),
                addr_len,
                parent_st,
                child_st,
                entry.prot,
                entry.flags,
                entry.backing,
            );
        }
        
        let hits_target = {
            const TARGET: u64 = 0xffffe000;
            let start = addr_st as u64;
            let end = start + addr_len as u64;
            start <= TARGET && TARGET < end
        };

        if entry.flags & (MAP_SHARED as i32) != 0 {
            unsafe {
                let parent_value_before =
                    std::ptr::read_volatile(parent_st as *const u32);
                let child_value_before =
                    std::ptr::read_volatile(child_st as *const u32);

                if hits_target {
                    eprintln!(
                        "[fork-shared-before] cage={}->{} \
                        user={:#x} len={:#x} \
                        parent={:#x} child={:#x} \
                        parent_value={} child_value={} \
                        prot={:#x} flags={:#x} backing={:?}",
                        parent_cageid,
                        child_cageid,
                        addr_st,
                        addr_len,
                        parent_st,
                        child_st,
                        parent_value_before,
                        child_value_before,
                        entry.prot,
                        entry.flags,
                        entry.backing,
                    );
                }

                // Clear errno so that it belongs to this mremap call.
                *libc::__errno_location() = 0;

                let ret = libc::mremap(
                    parent_st as *mut libc::c_void,
                    0,
                    addr_len,
                    (MREMAP_MAYMOVE | MREMAP_FIXED) as i32,
                    child_st as *mut libc::c_void,
                );

                let errno = *libc::__errno_location();

                let parent_value_after =
                    std::ptr::read_volatile(parent_st as *const u32);
                let child_value_after =
                    std::ptr::read_volatile(child_st as *const u32);

                if hits_target {
                    eprintln!(
                        "[fork-shared-after] ret={:?} errno={} \
                        parent_value={} child_value={} \
                        same_address={}",
                        ret,
                        errno,
                        parent_value_after,
                        child_value_after,
                        parent_st == child_st,
                    );
                }

                if ret == libc::MAP_FAILED {
                    eprintln!(
                        "[fork-shared-error] mremap failed: {} \
                        user={:#x} parent={:#x} child={:#x}",
                        std::io::Error::from_raw_os_error(errno),
                        addr_st,
                        parent_st,
                        child_st,
                    );

                    // Aliasing the same physical page at two addresses is
                    // impossible on some platforms (e.g. inside an SGX enclave,
                    // where the EPCM binds each page to a single linear
                    // address). Fall back to copying the parent's contents so
                    // static metadata stored in the region (e.g. the `private`
                    // field of a pshared sem_t) stays valid in the child. The
                    // authoritative state of cross-cage sync objects does not
                    // live in this page: it is kept in rawposix, keyed by
                    // (shared region id, offset), so a stale copy of the value
                    // here is harmless.
                    let needs_write = entry.prot & PROT_WRITE == 0;
                    if needs_write {
                        let mret = libc::mprotect(
                            child_st as *mut libc::c_void,
                            addr_len,
                            entry.prot | PROT_WRITE,
                        );
                        assert_eq!(mret, 0, "failed to make child shared mapping writable");
                    }
                    std::ptr::copy_nonoverlapping(
                        parent_st as *const u8,
                        child_st as *mut u8,
                        addr_len,
                    );
                    if needs_write {
                        let mret = libc::mprotect(
                            child_st as *mut libc::c_void,
                            addr_len,
                            entry.prot,
                        );
                        assert_eq!(mret, 0, "failed to restore child shared mapping protection");
                    }
                }
            }
        } else {
            let needs_write = entry.prot & PROT_WRITE == 0;

            
            // eprintln!("[fork_vmmap] before writable mprotect");
            unsafe {
                // temporarily enable write on child's memory region to write parent data
                if needs_write {
                    let ret = libc::mprotect(
                        child_st as *mut libc::c_void,
                        addr_len,
                        entry.prot | PROT_WRITE,
                    );
                    assert_eq!(ret, 0, "failed to make child mapping writable");
                }

                // write parent data
                let local_iov = libc::iovec {
                    iov_base: parent_st as *mut libc::c_void,
                    iov_len: addr_len,
                };
                let remote_iov = libc::iovec {
                    iov_base: child_st as *mut libc::c_void,
                    iov_len: addr_len,
                };

                // eprintln!("[fork_vmmap] before process_vm_writev");
                let ret = libc::process_vm_writev(libc::getpid(), &local_iov, 1, &remote_iov, 1, 0);
                if ret < 0 {
                    lind_log!(
                        Default,
                        "process_vm_writev failed with errno {} (parent_st=0x{:x}, child_st=0x{:x}, len={}), falling back to copy_nonoverlapping",
                        get_errno(),
                        parent_st,
                        child_st,
                        addr_len,
                    );
                    // eprintln!(
                    //     "[fork_vmmap] after process_vm_writev ret={}, errno={}",
                    //     ret,
                    //     std::io::Error::last_os_error()
                    // );
                    std::ptr::copy_nonoverlapping(
                        parent_st as *const u8,
                        child_st as *mut u8,
                        addr_len,
                    );
                }

                // println!("[fork_vmmap] before restore mprotect");
                // revert child's memory region prot
                if needs_write {
                    let ret = libc::mprotect(
                        child_st as *mut libc::c_void,
                        addr_len,
                        entry.prot,
                    );
                    assert_eq!(ret, 0, "failed to restore child mapping protection");
                }
            };
        }
    }

    // update program break for child
    drop(child_vmmap);
    let mut child_vmmap = child_cage.vmmap.write();
    child_vmmap.set_heap_start(parent_vmmap.heap_start);
}

// set the wasm linear memory base address to vmmap
pub fn init_vmmap(cageid: u64, base_address: usize, heap_start: Option<u32>) {
    let cage = get_cage(cageid).unwrap();
    let mut vmmap = cage.vmmap.write();
    vmmap.set_base_address(base_address);
    if heap_start.is_some() {
        vmmap.set_heap_start(heap_start.unwrap());
    }
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
    cageid: u64,
    arg: u64,
    length: usize,
    prot: i32,
) -> Result<u64, Errno> {
    // search from the table and get the item from
    let cage = get_cage(cageid).unwrap();

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

/// This function translates a virtual memory address to a physical address by adding the base address
/// of the `vmmap` to the given argument. This translation is needed because the system uses a
/// virtualized address space within each cage, where guest-visible addresses are offsets from the
/// base of the cage’s allocated memory region. Adding the base address produces the actual physical
/// (host) address used for memory operations.
///
/// # Arguments
/// * `cage` - Reference to the memory cage containing the virtual memory map
/// * `arg` - Virtual memory address to translate
///
/// # Returns
/// * `Ok(u64)` - Translated physical memory address

pub fn translate_vmmap_addr(cage: &Cage, arg: u64) -> Result<u64, Errno> {
    // Get read lock on virtual memory map
    let vmmap = cage.vmmap.read();
    Ok(vmmap.base_address.unwrap() as u64 + arg)
}

/// Checks if a given address range is readable for a specific cage
///
/// This is a high-level wrapper that retrieves the cage's vmmap and checks
/// if the specified memory range has read permissions.
///
/// # Arguments
/// * `cageid` - The cage identifier
/// * `addr` - Virtual memory address to check
/// * `length` - Length of the memory region in bytes
///
/// # Returns
/// * `Ok(true)` - If the entire range is mapped and readable
/// * `Err(Errno::EINVAL)` - If the cage does not exist
/// * `Err(Errno::EFAULT)` - If any part of the range is unmapped or not readable
pub fn check_addr_read(cageid: u64, addr: u64, length: usize) -> Result<bool, Errno> {
    let cage = get_cage(cageid).ok_or(Errno::EINVAL)?;
    let mut vmmap = cage.vmmap.write();

    if vmmap.check_addr_read(addr, length) {
        Ok(true)
    } else {
        Err(Errno::EFAULT)
    }
}

/// Checks if a given address range is writable for a specific cage
///
/// This is a high-level wrapper that retrieves the cage's vmmap and checks
/// if the specified memory range has write permissions.
///
/// # Arguments
/// * `cageid` - The cage identifier
/// * `addr` - Virtual memory address to check
/// * `length` - Length of the memory region in bytes
///
/// # Returns
/// * `Ok(true)` - If the entire range is mapped and writable
/// * `Err(Errno::EINVAL)` - If the cage does not exist
/// * `Err(Errno::EFAULT)` - If any part of the range is unmapped or not writable
pub fn check_addr_write(cageid: u64, addr: u64, length: usize) -> Result<bool, Errno> {
    let cage = get_cage(cageid).ok_or(Errno::EINVAL)?;
    let mut vmmap = cage.vmmap.write();

    if vmmap.check_addr_write(addr, length) {
        Ok(true)
    } else {
        Err(Errno::EFAULT)
    }
}

/// Checks if a given address range is readable and writable for a specific cage
///
/// This is a high-level wrapper that retrieves the cage's vmmap and checks
/// if the specified memory range has both read and write permissions.
///
/// # Arguments
/// * `cageid` - The cage identifier
/// * `addr` - Virtual memory address to check
/// * `length` - Length of the memory region in bytes
///
/// # Returns
/// * `Ok(true)` - If the entire range is mapped with both read and write permissions
/// * `Err(Errno::EINVAL)` - If the cage does not exist
/// * `Err(Errno::EFAULT)` - If any part of the range is unmapped or lacks read/write permissions
pub fn check_addr_rw(cageid: u64, addr: u64, length: usize) -> Result<bool, Errno> {
    let cage = get_cage(cageid).ok_or(Errno::EINVAL)?;
    let mut vmmap = cage.vmmap.write();

    if vmmap.check_addr_rw(addr, length) {
        Ok(true)
    } else {
        Err(Errno::EFAULT)
    }
}
