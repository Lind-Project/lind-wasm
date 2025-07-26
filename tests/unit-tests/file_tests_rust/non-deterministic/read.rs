use std::fs::File;
use std::io::{self, Read, Write};

fn main() {
    // Try to open /dev/urandom, but handle the case where it's not available
    let mut file = match File::open("/dev/urandom") {
        Ok(file) => file,
        Err(e) => {
            eprintln!("open(): {}", e);
            // In WASM environment, /dev/urandom might not be available
            // Just print a message and exit gracefully
            println!("Random data not available in WASM environment");
            return;
        }
    };

    let mut buf = [0u8; 4096];
    let mut pos = 0;
    let count = buf.len();

    while pos < count {
        match file.read(&mut buf[pos..]) {
            Ok(ret) if ret > 0 => {
                println!("read() ret: [{}], left: [{}]", ret, count - pos);
                pos += ret;
            }
            Ok(0) => break, // EOF
            Ok(_) => break,
            Err(e) => {
                eprintln!("read(): {}", e);
                break; // Don't exit, just break
            }
        }
    }

    // Print printable characters
    for &byte in &buf {
        if byte.is_ascii_graphic() {
            io::stdout().write_all(&[byte]).unwrap();
        }
    }
    io::stdout().write_all(b"\n").unwrap();
} 