use std::io::{self, Write};

fn main() {
    // In Rust, we can use stdout directly
    let mut stdout = io::stdout();
    let str = "write succeeded\n";
    
    if let Ok(_) = stdout.write_all(str.as_bytes()) {
        stdout.flush().unwrap();
    }
    
    println!("Dup write test completed");
} 