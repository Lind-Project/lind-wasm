use lind_if::{Errno, HostAddress, LindError, LindMprotectArgs, LindMunmapArgs, LindResult};

pub(crate) fn last_error() -> LindError {
    let errno = std::io::Error::last_os_error()
        .raw_os_error()
        .expect("host memory operation failed without an errno");
    LindError::Other(
        Errno::from_discriminant(errno)
            .unwrap_or_else(|_| panic!("unknown host memory-operation errno: {errno}")),
    )
}

pub(crate) fn mmap_result(address: *mut libc::c_void) -> LindResult<HostAddress> {
    if address == libc::MAP_FAILED {
        Err(last_error())
    } else {
        Ok(address as HostAddress)
    }
}

pub(crate) fn mprotect(args: &LindMprotectArgs) -> LindResult<()> {
    let result = unsafe {
        libc::mprotect(
            args.address as *mut libc::c_void,
            args.length as usize,
            args.prot,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(last_error())
    }
}

pub(crate) fn munmap(args: &LindMunmapArgs) -> LindResult<()> {
    let result = unsafe { libc::munmap(args.address as *mut libc::c_void, args.length as usize) };
    if result == 0 {
        Ok(())
    } else {
        Err(last_error())
    }
}
