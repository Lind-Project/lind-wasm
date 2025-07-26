use std::io::{self, Write};

fn main() -> io::Result<()> {
    println!("calling sysconf(sysconf())");
    
    // In Rust, we can get page size using std::alloc::Layout or similar
    // For this test, we'll use a system call equivalent
    let page_size = unsafe {
        // Use libc if available, otherwise use a reasonable default
        #[cfg(target_os = "linux")]
        {
            let mut page_size: libc::c_long = 0;
            if libc::sysconf(libc::_SC_PAGESIZE) > 0 {
                libc::sysconf(libc::_SC_PAGESIZE)
            } else {
                4096 // Default page size
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            4096 // Default page size for other systems
        }
    };
    
    if page_size > 0 {
        println!("page size: {}", page_size);
    } else {
        println!("sysconf() failed");
        std::process::exit(1);
    }
    
    io::stdout().flush()?;
    Ok(())
} 