//! Host-process mapping primitives shared by backends that isolate cages inside the Lind
//! process address space (Wasmtime today). They implement the Lind-IF §7 effects with host
//! `mmap`/`mprotect`; `munmap` keeps the range reserved as `PROT_NONE` so the cage's
//! reservation is never released.

use crate::{HostAddress, LindError, LindMmapArgs, LindMprotectArgs, LindMunmapArgs, LindResult};
use sysdefs::constants::err_const::get_errno;

fn last_error() -> LindError {
    LindError::from_errno(get_errno() as i64)
}

pub fn mmap(args: LindMmapArgs) -> LindResult<HostAddress> {
    let result = unsafe {
        libc::mmap(
            args.addr as *mut libc::c_void,
            args.len as usize,
            args.prot,
            args.flags,
            args.fd,
            args.offset,
        )
    };
    if result == libc::MAP_FAILED {
        Err(last_error())
    } else {
        Ok(result as HostAddress)
    }
}

pub fn mprotect(args: LindMprotectArgs) -> LindResult<()> {
    let result =
        unsafe { libc::mprotect(args.addr as *mut libc::c_void, args.len as usize, args.prot) };
    if result < 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}

pub fn munmap(args: LindMunmapArgs) -> LindResult<()> {
    let result = unsafe {
        libc::mmap(
            args.addr as *mut libc::c_void,
            args.len as usize,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED,
            -1,
            0,
        )
    };
    if result as HostAddress != args.addr {
        sysdefs::lind_debug_panic!(
            "munmap: MAP_FIXED violation - mmap returned {:p} but requested {:#x}",
            result,
            args.addr
        );
    }
    if result == libc::MAP_FAILED {
        return Err(last_error());
    }
    Ok(())
}
