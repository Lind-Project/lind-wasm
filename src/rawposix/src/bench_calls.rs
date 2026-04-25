use fdtables;
use sysdefs::constants::Errno;
use typemap::datatype_conversion::sc_unusedarg;
use typemap::err_const::syscall_error;

use libc;

use crate::perf;

/// Special benchmark syscall for measuring a syscall that calls the kernel for resolution.
///
/// CALL_NUM: 2001
///
/// Simulates the behaviour of geteuid().
/// - publically exported through lind_syscall.h as `libc_syscall()`
pub extern "C" fn libc_syscall(
    cageid: u64,
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
) -> i32 {
    let _timer = lind_perf::get_timer!(perf::LIBC_CALL);

    // Validate that each extra argument is unused.
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

    (unsafe { libc::geteuid() }) as i32
}

/// Special benchmark syscall for measuring a syscall that does not get resolved through the
/// kernel.
///
/// CALL_NUM: 2002
///
/// Simulates the behaviour of close(-1) i.e closing an invalid file descriptor.
/// - vfd_arg is hardcoded to -1 through glibc.
/// - publically exported through lind_syscall.h as `fdtables_syscall()`
pub extern "C" fn fdtables_syscall(
    cageid: u64,
    vfd_arg: u64,
    vfd_cageid: u64,
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
) -> i32 {
    let _timer = lind_perf::get_timer!(perf::FDTABLES_CALL);

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

    // Call close(-1)
    match fdtables::close_virtualfd(cageid, vfd_arg) {
        Ok(()) => 0,
        Err(e) => {
            if e == Errno::EBADFD as u64 {
                syscall_error(Errno::EBADF, "close", "Bad File Descriptor")
            } else if e == Errno::EINTR as u64 {
                syscall_error(Errno::EINTR, "close", "Interrupted system call")
            } else {
                syscall_error(Errno::EIO, "close", "I/O error")
            }
        }
    }
}
