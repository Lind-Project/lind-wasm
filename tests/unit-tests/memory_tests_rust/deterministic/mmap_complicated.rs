use std::alloc::{alloc, dealloc, Layout};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    // Define the size of the shared memory
    let mem_size = 1024;
    
    // Create shared memory region using Rust's allocator (simulate mmap)
    let layout = Layout::from_size_align(mem_size, 8).unwrap();
    let shared_mem = unsafe { alloc(layout) };
    
    if shared_mem.is_null() {
        println!("mmap failed");
        return Ok(());
    }
    
    // Use Arc<Mutex> to simulate shared memory between threads
    let shared_data = Arc::new(Mutex::new(Vec::<u8>::new()));
    let shared_data_clone = shared_data.clone();
    
    // Spawn a child thread (simulate fork)
    let child_handle = thread::spawn(move || {
        // Child thread
        println!("Child: Writing to shared memory.");
        let child_message = b"Hello from the child process!";
        
        if let Ok(mut data) = shared_data_clone.lock() {
            data.clear();
            data.extend_from_slice(child_message);
        }
        
        // Sleep to simulate some work
        thread::sleep(Duration::from_secs(1));
        
        if let Ok(data) = shared_data_clone.lock() {
            let content = std::str::from_utf8(&data).unwrap_or("Invalid UTF-8");
            println!("Child: Reading from shared memory: '{}'", content);
        }
        
        println!("Child: Exiting.");
    });
    
    // Parent thread
    println!("Parent: Waiting for child to write.");
    
    // Sleep to simulate waiting for the child
    thread::sleep(Duration::from_millis(500));
    
    if let Ok(data) = shared_data.lock() {
        let content = std::str::from_utf8(&data).unwrap_or("Invalid UTF-8");
        println!("Parent: Reading from shared memory: '{}'", content);
    }
    
    let parent_message = b"Hello from the parent process!";
    if let Ok(mut data) = shared_data.lock() {
        data.clear();
        data.extend_from_slice(parent_message);
    }
    
    // Wait for the child to finish
    let _ = child_handle.join();
    
    if let Ok(data) = shared_data.lock() {
        let content = std::str::from_utf8(&data).unwrap_or("Invalid UTF-8");
        println!("Parent: Reading modified shared memory: '{}'", content);
    }
    
    // Clean up (simulate munmap)
    unsafe { dealloc(shared_mem, layout) };
    
    println!("Parent: Exiting.");
    Ok(())
} 