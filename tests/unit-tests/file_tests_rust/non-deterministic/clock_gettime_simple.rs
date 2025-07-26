use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Get current time using SystemTime
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let seconds = duration.as_secs();
            let nanoseconds = duration.subsec_nanos();
            println!("Current time: {} seconds and {} nanoseconds", seconds, nanoseconds);
        }
        Err(e) => {
            eprintln!("clock_gettime failed: {}", e);
        }
    }
} 