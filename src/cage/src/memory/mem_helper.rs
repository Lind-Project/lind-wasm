//! VMMAP helper functions
//!
//! This file provides helper functions related to vmmap, including address alignment,
//! initializing vmmap, helper functions for handling vmmap during a fork syscall, and
//! address translation and validation related to vmmap
use crate::cage::{get_cage, Cage};
use crate::memory::{MemoryBackingType, Vmmap, VmmapOps};
use libc::c_void;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{
    F_GETFL, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, MREMAP_FIXED, MREMAP_MAYMOVE,
    PAGESHIFT, PAGESIZE, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE,
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
                    (MREMAP_MAYMOVE | MREMAP_FIXED) as i32,
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

// set the wasm linear memory base address to vmmap
pub fn init_vmmap_helper(cageid: u64, base_address: usize, program_break: Option<u32>) {
    println!("Cageid: {}", cageid);
    let cage = get_cage(cageid).unwrap();
    let mut vmmap = cage.vmmap.write();
    vmmap.set_base_address(base_address);
    if program_break.is_some() {
        vmmap.set_program_break(program_break.unwrap());
    }
}

// clone the cage memory. Invoked by wasmtime after cage is forked
pub fn fork_vmmap_helper(parent_cageid: u64, child_cageid: u64) {
    let parent_cage = get_cage(parent_cageid).unwrap();
    let child_cage = get_cage(child_cageid).unwrap();
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
    Ok(true)
}

/// This function translates a virtual memory address to a physical address by adding the base address of the vmmap to the argument.
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
