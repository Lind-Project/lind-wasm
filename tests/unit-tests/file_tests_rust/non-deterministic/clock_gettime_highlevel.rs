use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() {
    // Get the start time using Instant
    let begin = Instant::now();
    let start_system_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    println!("Running 1,000,000 iterations...");

    let mut sum: i64 = 0;
    for i in 0..1_000_000 {
        sum += i;
    }

    // Get the end time
    let end = Instant::now();
    let end_system_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Calculate elapsed time
    let elapsed_time = end.duration_since(begin);

    // Display results
    println!("\nStart time: {} clock ticks", start_system_time);
    println!("End time: {} clock ticks", end_system_time);
    println!("Elapsed CPU time: {:.9} seconds", elapsed_time.as_secs_f64());
} 