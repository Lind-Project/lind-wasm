use std::io::{self, Write};

fn main() {
    let str = "write succeeded\n";
    
    // Write only the first 5 bytes to match C behavior exactly
    let mut stdout = io::stdout();
    let _ = stdout.write_all(&str.as_bytes()[..5]);
    stdout.flush().unwrap();
    
    // C version doesn't print anything else, so we shouldn't either
} 