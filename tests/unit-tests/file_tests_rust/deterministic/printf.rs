use std::io::{self, Write};

fn main() {
    print!("Hello, World!");
    io::stdout().flush().unwrap();
    println!(); // Add newline to match C printf behavior
} 