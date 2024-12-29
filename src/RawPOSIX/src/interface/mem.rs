use crate::constants::{
    F_GETFL, MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, PROT_EXEC
};

use crate::safeposix::vmmap::{MemoryBackingType, Vmmap, VmmapOps};
use crate::constants::{MAP_SHARED, PROT_NONE, PROT_READ, PROT_WRITE, PAGESHIFT, PAGESIZE};

use crate::interface::{cagetable_getref, syscall_error, Errno};
use crate::safeposix::cage::Cage;
use std::result::Result;

// heap is placed at the very top of the memory
pub const HEAP_ENTRY_INDEX: u32 = 0;

pub fn round_up_page(length: u64) -> u64 {
    if length % PAGESIZE as u64 == 0 {
        length
    } else {
        ((length / PAGESIZE as u64) + 1) * PAGESIZE as u64
    }
}

pub fn fork_vmmap(parent_vmmap: &Vmmap, child_vmmap: &Vmmap) {
    let parent_base = parent_vmmap.base_address.unwrap();
    let child_base = child_vmmap.base_address.unwrap();

    // iterate through each vmmap entry
    for (_interval, entry) in parent_vmmap.entries.iter() {
        // if the entry has PROT_NONE, that means the entry is currently not used
        if entry.prot == PROT_NONE { continue; }
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
            let result = unsafe { libc::mremap(parent_st as *mut libc::c_void, 0, addr_len, libc::MREMAP_MAYMOVE | libc::MREMAP_FIXED, child_st as *mut libc::c_void) };
        } else {
            unsafe {
                // temporarily enable write on child's memory region to write parent data
                libc::mprotect(child_st as *mut libc::c_void, addr_len, PROT_READ | PROT_WRITE);

                // write parent data
                // TODO: replace copy_nonoverlapping with writev for potential performance boost
                std::ptr::copy_nonoverlapping(parent_st as *const u8, child_st as *mut u8, addr_len);

                // revert child's memory region prot
                libc::mprotect(child_st as *mut libc::c_void, addr_len, entry.prot)
            };
        }
    }
}

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
    let result = cage.mmap_syscall(sysaddr as *mut u8, rounded_length, PROT_NONE, (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32, -1, 0);
    if result != sysaddr {
        panic!("MAP_FIXED not fixed");
    }

    let mut vmmap = cage.vmmap.write();

    vmmap.remove_entry(rounded_addr as u32 >> PAGESHIFT, len as u32 >> PAGESHIFT);

    0
}

pub fn mmap_handler(cageid: u64, addr: *mut u8, len: usize, mut prot: i32, mut flags: i32, mut fildes: i32, off: i64) -> u32 {
    let cage = cagetable_getref(cageid);

    let mut maxprot = PROT_READ | PROT_WRITE;

    // only these four flags are allowed
    let allowed_flags = MAP_FIXED as i32 | MAP_SHARED as i32 | MAP_PRIVATE as i32 | MAP_ANONYMOUS as i32;
    if flags & !allowed_flags > 0 {
        // truncate flag to remove flags that are not allowed
        flags &= allowed_flags;
    }

    if prot & PROT_EXEC > 0 {
        println!("mmap syscall error 1!");
        return syscall_error(Errno::EINVAL, "mmap", "PROT_EXEC is not allowed") as u32;
    }

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64);
    if rounded_addr != addr as u64 {
        println!("mmap syscall error 2!");
        return syscall_error(Errno::EINVAL, "mmap", "address it not aligned") as u32;
    }

    // offset should be non-negative and multiple of pages
    if off < 0 {
        println!("mmap syscall error 3!");
        return syscall_error(Errno::EINVAL, "mmap", "offset cannot be negative") as u32;
    }
    let rounded_off = round_up_page(off as u64);
    if rounded_off != off as u64 {
        println!("mmap syscall error 4!");
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
            println!("find map space result: {:?}", result);
        } else {
            // use address user provided as hint to find address
            result = vmmap.find_map_space_with_hint(rounded_length as u32 >> PAGESHIFT, 1, addr as u32);
        }

        // did not find desired memory region
        if result.is_none() {
            return syscall_error(Errno::ENOMEM, "mmap", "no memory") as u32;
        }

        let space = result.unwrap();
        useraddr = (space.start() << PAGESHIFT) as u32;
    }

    // TODO: validate useraddr (like checking whether within the program break)

    flags |= MAP_FIXED as i32;

    // either MAP_PRIVATE or MAP_SHARED should be set, but not both
    if (flags & MAP_PRIVATE as i32 == 0) == (flags & MAP_SHARED as i32 == 0) {
        return syscall_error(Errno::EINVAL, "mmap", "invalid flags") as u32;
    }

    let vmmap = cage.vmmap.read();

    let sysaddr = vmmap.user_to_sys(useraddr);
    println!("useraddr: {}, sysaddr: {}", useraddr, sysaddr);

    drop(vmmap);

    if rounded_length > 0 {
        if flags & MAP_ANONYMOUS as i32 > 0 {
            fildes = -1;
        }

        let result = cage.mmap_syscall(sysaddr as *mut u8, rounded_length as usize, prot, flags, fildes, off);
        
        let vmmap = cage.vmmap.read();
        println!("sys addr: {}", result);
        let result = vmmap.sys_to_user(result);
        println!("user addr: {}", result);
        drop(vmmap);

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
                        return syscall_error(Errno::EINVAL, "mmap", "invalid file descriptor") as u32;
                    }
                    maxprot &= flags;
                    MemoryBackingType::FileDescriptor(fildes as u64)
                }
            };

            vmmap.add_entry_with_overwrite(useraddr >> PAGESHIFT, (rounded_length >> PAGESHIFT) as u32, prot, maxprot, flags, backing, off, len as i64, cageid);
        }
    }

    useraddr as u32
}

pub fn sbrk_handler(cageid: u64, brk: i32) -> u32 {
    let cage = cagetable_getref(cageid);

    // get the heap entry
    let mut vmmap = cage.vmmap.read();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    assert!(heap.npages == vmmap.program_break);

    if brk == 0 {
        return (PAGESIZE * heap.npages) as u32;
    }

    // round up the break to multiple of pages
    let brk_page;
    if brk < 0 {
        brk_page = -((round_up_page(-brk as u64) >> PAGESHIFT) as i32);
    } else {
        brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as i32;
    }

    drop(vmmap);

    if brk_handler(cageid, ((heap.npages as i32 + brk_page) << PAGESHIFT) as u32) < 0 {
        return syscall_error(Errno::ENOMEM, "sbrk", "no memory") as u32;
    }

    (PAGESIZE * heap.npages) as u32
}

pub fn brk_handler(cageid: u64, brk: u32) -> i32 {
    let cage = cagetable_getref(cageid);

    let mut vmmap = cage.vmmap.write();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    assert!(heap.npages == vmmap.program_break);

    let old_brk_page = heap.npages;
    // round up the break to multiple of pages
    let brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as u32;

    // TODO: check if brk has enough space

    vmmap.add_entry_with_overwrite(0, brk_page, heap.prot, heap.maxprot, heap.flags, heap.backing, heap.file_offset, heap.file_size, heap.cage_id);
    
    let old_heap_end_usr = (old_brk_page * PAGESIZE) as u32;
    let old_heap_end_sys = vmmap.user_to_sys(old_heap_end_usr)as *mut u8;

    let new_heap_end_usr = (brk_page * PAGESIZE) as u32;
    let new_heap_end_sys = vmmap.user_to_sys(new_heap_end_usr)as *mut u8;

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
            0
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
            0
        );
        
        if ret < 0 {
            panic!("brk mmap failed");
        }
    }

    0
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
pub fn check_and_convert_addr_ext(cage: &Cage, arg: u64, length: usize, prot: i32) -> Result<u64, Errno> {
    // Get read lock on virtual memory map
    // TODO: need to add change here based on the protection, currently fixed for build error
    let mut vmmap = cage.vmmap.write();
    
    // Calculate page numbers for start and end of region
    let page_num = (arg >> PAGESHIFT) as u32;  // Starting page number
    let end_page = ((arg + length as u64 + PAGESIZE as u64 - 1) >> PAGESHIFT) as u32;  // Ending page number (rounded up)
    let npages = end_page - page_num;  // Total number of pages spanned
    
    // Validate memory mapping and permissions
    if vmmap.check_addr_mapping(page_num, npages, prot).is_none() {
        println!("invalid address: {}", arg);
        return Err(Errno::EFAULT);  // Return error if mapping invalid
    }

    // Convert to physical address by adding base address
    Ok(vmmap.base_address.unwrap() as u64 + arg)
}
