use std::io::{self, Write};

fn main() -> io::Result<()> {
    let buffer_size = 256;
    let mut host_buffer = vec![0u8; buffer_size];
    
    // Get hostname using std::env::var or similar
    // Note: In Rust, we typically use std::env::var("HOSTNAME") or similar
    // For this test, we'll simulate the behavior
    match std::env::var("HOSTNAME") {
        Ok(hostname) => {
            println!("Hostname: {}", hostname);
        }
        Err(_) => {
            // Fallback: try to get hostname from system
            if let Ok(hostname) = std::process::Command::new("hostname")
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            {
                println!("Hostname: {}", hostname);
            } else {
                println!("Hostname: unknown");
            }
        }
    }
    
    io::stdout().flush()?;
    Ok(())
} 