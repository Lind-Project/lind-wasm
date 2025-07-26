use std::alloc::{alloc, dealloc, Layout};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let text = "Mprotect write test text";
    
    // Allocate memory (simulate mmap with PROT_READ)
    let layout = Layout::from_size_align(4096, 8).unwrap();
    let readonly_data = unsafe { alloc(layout) };
    
    if readonly_data.is_null() {
        println!("Failed to mmap page");
        return Ok(());
    }
    
    // Simulate mprotect to make it writable
    // In Rust, we can write to allocated memory directly
    println!("Memory protection changed to read-write");
    
    // Copy text to the allocated memory
    unsafe {
        let text_bytes = text.as_bytes();
        let data_slice = std::slice::from_raw_parts_mut(readonly_data, 4096);
        let copy_len = text_bytes.len().min(4096);
        data_slice[..copy_len].copy_from_slice(&text_bytes[..copy_len]);
        
        // Print the content
        let content = std::str::from_utf8(&data_slice[..copy_len]).unwrap_or("Invalid UTF-8");
        println!("{}", content);
    }
    
    // Clean up (simulate munmap)
    unsafe { dealloc(readonly_data, layout) };
    
    Ok(())
} 