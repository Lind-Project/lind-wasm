use std::io::{self, Write};

fn main() {
    let str = "write succeeded\n";
    
    for byte in str.bytes() {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(&[byte]);
        stdout.flush().unwrap();
    }
    
    println!("Write loop test completed");
} 