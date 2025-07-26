use std::env;

fn main() {
    match env::current_dir() {
        Ok(path) => {
            let cwd = path.to_string_lossy();
            println!("current working directory is: {} :: {}", cwd, cwd);
        }
        Err(e) => {
            eprintln!("getcwd() error: {}", e);
        }
    }
} 