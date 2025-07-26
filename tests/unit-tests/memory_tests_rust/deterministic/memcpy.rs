use std::io::{self, Write};

fn main() -> io::Result<()> {
    // Source and destination buffers
    let src = "Hello, World!";
    let mut dst = [0u8; 50];
    
    // Use Rust's copy_from_slice to copy data from src to dst
    let src_bytes = src.as_bytes();
    dst[..src_bytes.len()].copy_from_slice(src_bytes);
    
    // Use stdout to write the dst buffer (equivalent to write syscall)
    io::stdout().write_all(&dst[..src_bytes.len()])?;
    io::stdout().flush()?;
    
    Ok(())
} 