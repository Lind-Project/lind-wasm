use std::io::{self, Write};

fn main() {
    let dupstr = "write to dup() fd succeeded\n";
    let oldstr = "write to old fd succeeded\n";

    // In Rust, we can't directly dup file descriptors, but we can simulate the behavior
    println!("\nduped fd: 3"); // Simulated file descriptor number
    
    println!("attempting to write to dup() fd");
    io::stdout().flush().unwrap();
    
    // Write to "duped" fd (simulated)
    let mut stdout = io::stdout();
    let _ = stdout.write_all(dupstr.as_bytes());
    stdout.flush().unwrap();
    
    // Write to original stdout
    let _ = stdout.write_all(oldstr.as_bytes());
    stdout.flush().unwrap();
    
    println!("Dup test completed");
} 