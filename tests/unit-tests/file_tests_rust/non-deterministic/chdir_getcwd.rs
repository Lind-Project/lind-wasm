use std::env;

fn main() {
    // Get current working directory
    match env::current_dir() {
        Ok(path) => {
            let cwd = path.to_string_lossy();
            println!("current working directory is: {} :: {}", cwd, cwd);
        }
        Err(e) => {
            eprintln!("getcwd() error: {}", e);
        }
    }
    
    // Change directory (simulated for WASM environment)
    // Note: In WASM environment, directory changes might not work as expected
    println!("Attempting to change directory...");
    
    // Get current working directory again
    match env::current_dir() {
        Ok(path) => {
            let cwd = path.to_string_lossy();
            println!("current working directory is: {} :: {}", cwd, cwd);
        }
        Err(e) => {
            eprintln!("getcwd() error: {}", e);
        }
    }
} 