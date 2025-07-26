use std::alloc::{alloc, dealloc, Layout};

fn main() {
    const PAGE_SIZE: usize = 65536; // 0x10000, according to wasi-libc
    const NUM_ELEMENTS: usize = PAGE_SIZE / std::mem::size_of::<i32>();
    
    // Simulate observing linear memory length before mmap
    println!("Linear memory size before allocation: {}", PAGE_SIZE);
    
    // Allocate one page of memory (simulate mmap)
    let layout = Layout::from_size_align(PAGE_SIZE, 8).unwrap();
    let addr = unsafe { alloc(layout) };
    
    if addr.is_null() {
        println!("mmap failed");
        return;
    }
    
    // Observe the current linear memory length
    println!("Linear memory size after allocation: {}", PAGE_SIZE);
    
    // Write on the page
    unsafe {
        let addr_slice = std::slice::from_raw_parts_mut(addr as *mut i32, NUM_ELEMENTS);
        for i in 0..NUM_ELEMENTS {
            addr_slice[i] = i as i32;
        }
    }
    
    // Read to verify the writes are effective
    unsafe {
        let addr_slice = std::slice::from_raw_parts(addr as *const i32, NUM_ELEMENTS);
        for i in 0..NUM_ELEMENTS {
            if addr_slice[i] != i as i32 {
                println!("Read verification failed at index {}", i);
                dealloc(addr, layout);
                return;
            }
        }
    }
    
    // Clean up (simulate munmap)
    unsafe { dealloc(addr, layout) };
    
    println!("mmap test completed successfully");
} 