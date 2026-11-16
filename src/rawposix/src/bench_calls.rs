//! Benchmark-only syscalls (perf builds only).
//!
//! These give the lind-perf benchmarks two fixed, minimal paths through RawPOSIX:
//! - 2001 `libc_syscall`: resolved by the Linux kernel (behaves like `geteuid()`).
//! - 2002 `fdtables_syscall`: resolved inside lind by fdtables (behaves like `close(-1)`).
//!
//! The guest-side wrappers live in `tests/benchmarks/lind-perf/lind_perf_bench.h`.
use sysdefs::constants::err_const::{syscall_error, Errno};
use typemap::datatype_conversion::sc_unusedarg;

pub const LIBC_SYSCALL: u64 = 2001;
pub const FDTABLES_SYSCALL: u64 = 2002;

/// 2001: argument checks, then `geteuid()` in the kernel.
pub extern "C" fn libc_syscall(
    _cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i64 {
    perf_timer!(_t = LIBC_CALL);

    if !(sc_unusedarg(arg1, arg1_cageid)
        && sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "libc_syscall"
        );
    }

    (unsafe { libc::geteuid() }) as i64
}

/// 2002: argument checks, then `close_virtualfd` on the fd passed by the guest (always -1, so
/// fdtables rejects it without touching the kernel), then errno conversion. Mirrors
/// `close_syscall`.
pub extern "C" fn fdtables_syscall(
    cageid: u64,
    vfd_arg: u64,
    _vfd_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i64 {
    perf_timer!(_t = FDTABLES_CALL);

    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        panic!(
            "{}: unused arguments contain unexpected values -- security violation",
            "fdtables_syscall"
        );
    }

    perf_timer!(_t_fdt = CLOSE_VIRTUALFD);
    let res = fdtables::close_virtualfd(cageid, vfd_arg);
    perf_stop!(_t_fdt);

    match res {
        Ok(()) => 0,
        Err(e) => {
            (if e == Errno::EBADFD as u64 {
                syscall_error(Errno::EBADF, "close", "Bad File Descriptor")
            } else if e == Errno::EINTR as u64 {
                syscall_error(Errno::EINTR, "close", "Interrupted system call")
            } else {
                syscall_error(Errno::EIO, "close", "I/O error")
            }) as i64
        }
    }
}
