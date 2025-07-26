use std::fs::{File, OpenOptions};
use std::io::{self, Write};

const FILE_PATH: &str = "close.txt";
const ITERATIONS: i32 = 2000;

fn main() {
    // Create the test file
    let file_result = OpenOptions::new()
        .create(true)
        .write(true)
        .open(FILE_PATH);
    
    if let Ok(file) = file_result {
        drop(file); // Close the file
    } else {
        eprintln!("Failed to create test file: {}", file_result.unwrap_err());
    }

    for _i in 0..ITERATIONS {
        let file = match File::open(FILE_PATH) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to open file: {}", e);
                // Don't exit, just continue
                continue;
            }
        };
        drop(file); // Close the file
    }

    println!("File opened and closed {} times successfully.", ITERATIONS);
    io::stdout().flush().unwrap();

    // Cleanup: remove the test file
    if let Err(e) = std::fs::remove_file(FILE_PATH) {
        eprintln!("Failed to remove test file: {}", e);
        // Don't exit on cleanup failure
    }
} 