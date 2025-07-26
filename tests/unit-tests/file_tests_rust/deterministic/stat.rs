use std::env;
use std::fs;
use std::io::{self, Write};

const FILENAME: &str = "testfiles/statfile.txt";

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("running stat(\"{}\")", FILENAME);
        match fs::metadata(FILENAME) {
            Ok(metadata) => {
                println!("size: {}", metadata.len());
            }
            Err(e) => {
                eprintln!("stat: {}", e);
                println!("errno: {}", e.raw_os_error().unwrap_or(-1));
                // Don't exit on error, just report it
            }
        }
        return;
    }

    for i in 1..args.len() {
        println!("running stat(\"{}\")", args[i]);
        match fs::metadata(&args[i]) {
            Ok(metadata) => {
                println!("size: {}", metadata.len());
            }
            Err(e) => {
                eprintln!("stat: {}", e);
                println!("errno: {}", e.raw_os_error().unwrap_or(-1));
                // Don't exit on error, just report it
            }
        }
    }
} 