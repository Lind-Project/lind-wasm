use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

fn main() {
    // Small chunks should use sbrk only (simulated with standard allocation)
    let layout_small = Layout::from_size_align(0x10000, 8).unwrap();
    let buf_small = unsafe { alloc(layout_small) };
    
    if buf_small.is_null() {
        println!("Failed to allocate small buffer");
        return;
    }
    
    // Try write/read on the allocated memory
    unsafe {
        *(buf_small as *mut i32) = 10;
        let my_int = *(buf_small as *const i32);
        println!("Small allocation test: {}", my_int);
    }
    
    // Deallocate small buffer
    unsafe { dealloc(buf_small, layout_small) };
    
    // Medium allocation
    let layout_medium = Layout::from_size_align(0x100, 8).unwrap();
    let buf_medium = unsafe { alloc(layout_medium) };
    
    if buf_medium.is_null() {
        println!("Failed to allocate medium buffer");
        return;
    }
    
    // Try write/read on the allocated memory
    unsafe {
        *(buf_medium as *mut i32) = 10;
        let my_int = *(buf_medium as *const i32);
        println!("Medium allocation test: {}", my_int);
    }
    
    // Deallocate medium buffer
    unsafe { dealloc(buf_medium, layout_medium) };
    
    // Larger chunks should trigger the mmap path of malloc (simulated)
    let layout_large = Layout::from_size_align(0x100000, 8).unwrap();
    let buf_large = unsafe { alloc(layout_large) };
    
    if buf_large.is_null() {
        println!("Failed to allocate large buffer");
        return;
    }
    
    // Try accessing it
    unsafe {
        *(buf_large as *mut i32) = 12;
        let my_int = *(buf_large as *const i32);
        println!("Large allocation test: {}", my_int);
    }
    
    // Deallocate large buffer
    unsafe { dealloc(buf_large, layout_large) };
    
    println!("All memory allocation tests completed successfully");
} 