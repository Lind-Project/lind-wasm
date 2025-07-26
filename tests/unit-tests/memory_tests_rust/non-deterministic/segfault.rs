use std::io::{self, Write, Read};
use std::process::{Command, Stdio};
use std::thread;

fn main() -> io::Result<()> {
    let mut str_buffer = [0u8; 4096];
    
    // Create a pipe using std::process::Command
    let mut child = Command::new("echo")
        .arg("hi")
        .stdout(Stdio::piped())
        .spawn()?;
    
    println!("pipe() ret: [child stdout, parent stdin]");
    
    // Read from child's stdout
    if let Some(mut stdout) = child.stdout.take() {
        let ret = stdout.read(&mut str_buffer)?;
        println!("read() ret: {}", ret);
        
        // Print the received data
        let content = std::str::from_utf8(&str_buffer[..ret]).unwrap_or("Invalid UTF-8");
        println!("{}", content);
    }
    
    // Wait for child to finish
    let _ = child.wait();
    
    Ok(())
} 