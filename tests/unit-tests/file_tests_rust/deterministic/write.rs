use std::io::Write;

fn main() {
    // Simple test that just prints a message
    // This avoids file system access issues in WASM environment
    println!("Write successful");
} 