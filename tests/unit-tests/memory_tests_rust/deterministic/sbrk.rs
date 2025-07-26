use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

fn main() {
    // Simulate sbrk behavior using Rust's allocator
    let size = 1024; // Allocate 1024 bytes
    
    // Get initial memory layout (simulate getting current program break)
    let layout = Layout::from_size_align(size, 8).unwrap();
    
    // Allocate memory (simulate sbrk allocation)
    let buffer = unsafe { alloc(layout) };
    
    if buffer.is_null() {
        println!("sbrk failed");
        return;
    }
    
    println!("Initial program break: {:p}", buffer);
    println!("New program break after allocation: {:p}", unsafe { buffer.add(size) });
    
    // Use the allocated memory
    unsafe {
        let buffer_slice = std::slice::from_raw_parts_mut(buffer, size);
        let message = b"Hello, sbrk memory!";
        let message_len = message.len().min(size);
        buffer_slice[..message_len].copy_from_slice(&message[..message_len]);
        
        // Print the content
        let content = std::str::from_utf8(&buffer_slice[..message_len]).unwrap_or("Invalid UTF-8");
        println!("Content in allocated memory: {}", content);
    }
    
    // Deallocate memory (simulate moving program break back)
    unsafe { dealloc(buffer, layout) };
    
    println!("Program break after deallocation: memory freed");
    
    println!("sbrk test completed successfully");
} 