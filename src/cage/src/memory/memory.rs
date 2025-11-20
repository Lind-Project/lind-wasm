//! VMMAP helper functions
//!
//! This file provides helper functions related to vmmap, including address alignment,
//! initializing vmmap, helper functions for handling vmmap during a fork syscall, and
//! address translation and validation related to vmmap
use crate::cage::{get_cage, Cage};
use crate::memory::VmmapOps;
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::fs_const::{
    MAP_SHARED, MREMAP_FIXED, MREMAP_MAYMOVE, PAGESHIFT, PAGESIZE, PROT_READ, PROT_WRITE,
};

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
    panic!(
        "mmap returned unaligned address outside errno range: 0x{:x}",
        ret
    );
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
///    - The function uses `std::ptr::copy_nonoverlapping` to copy the memory contents directly.
///    - **TODO**: Investigate whether using `writev` could improve performance for this case.
///
/// # Arguments
/// * `parent_cageid` - cageid of parent
/// * `child_cageid` - caegid of child
pub fn fork_vmmap(parent_cageid: u64, child_cageid: u64) {
    // first retrieve corresponding vmmaps
    let parent_cage = get_cage(parent_cageid).unwrap();
    let child_cage = get_cage(child_cageid).unwrap();
    let parent_vmmap = parent_cage.vmmap.read();
    let child_vmmap = child_cage.vmmap.read();

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
            unsafe {
                libc::mremap(
                    parent_st as *mut libc::c_void,
                    0,
                    addr_len,
                    (MREMAP_MAYMOVE | MREMAP_FIXED) as i32,
                    child_st as *mut libc::c_void,
                );
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
                libc::mprotect(child_st as *mut libc::c_void, addr_len, entry.prot);
            };
        }
    }

    // update program break for child
    drop(child_vmmap);
    let mut child_vmmap = child_cage.vmmap.write();
    child_vmmap.set_program_break(parent_vmmap.program_break);
}

// set the wasm linear memory base address to vmmap
pub fn init_vmmap(cageid: u64, base_address: usize, program_break: Option<u32>) {
    let cage = get_cage(cageid).unwrap();
    let mut vmmap = cage.vmmap.write();
    vmmap.set_base_address(base_address);
    if program_break.is_some() {
        vmmap.set_program_break(program_break.unwrap());
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

pub fn check_addr(cageid: u64, arg: u64, length: usize, prot: i32) -> Result<bool, Errno> {
    // search from the table and get the item from
    let cage = get_cage(cageid).unwrap();

    // Get write lock on virtual memory map
    let mut vmmap = cage.vmmap.write();

    // Calculate page numbers for start and end of region
    let page_num = (arg >> PAGESHIFT) as u32; // Starting page number
    let end_page = ((arg + length as u64 + PAGESIZE as u64 - 1) >> PAGESHIFT) as u32; // Ending page number (rounded up)
    let npages = end_page - page_num; // Total number of pages spanned

    // Validate memory mapping and permissions
    if vmmap.check_addr_mapping(page_num, npages, prot).is_none() {
        return Err(Errno::EFAULT); // Return error if mapping invalid
    }
    Ok(true)
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
