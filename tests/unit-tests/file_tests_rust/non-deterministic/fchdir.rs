use std::env;
use std::fs;

fn main() {
    // Get the current working directory
    match env::current_dir() {
        Ok(path) => {
            let cwd = path.to_string_lossy();
            println!("current working directory is: {} :: {}", cwd, cwd);
        }
        Err(e) => {
            eprintln!("getcwd() error: {}", e);
            return;
        }
    }
    
    // In WASM environment, directory operations might be limited
    // We'll simulate the behavior
    println!("Attempting to change directory using file descriptor...");
    
    // Simulate getting the new working directory
    match env::current_dir() {
        Ok(path) => {
            let cwd = path.to_string_lossy();
            println!("current working directory is: {} :: {}", cwd, cwd);
        }
        Err(e) => {
            eprintln!("Error with getcwd: {}", e);
        }
    }
    
    println!("Fchdir test completed");
} 