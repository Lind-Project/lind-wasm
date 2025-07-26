use std::fs::File;
use std::io::Write;

fn main() {
    // Create a file and close it twice (second close should be safe)
    let file = File::create("testfiles/doubleclosefile.txt");
    if let Ok(mut file) = file {
        let _ = file.write_all(b"test");
        drop(file); // First close
        // Second close - this should be safe in Rust
        // The file is already dropped, so this is a no-op
    }
    
    println!("Double close test completed");
} 