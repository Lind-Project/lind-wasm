//! lind-perf counters for RawPOSIX (perf builds only).
use lind_perf::Counter;

/// Whole `libc_syscall` handler (2001): argument checks + real `geteuid()` kernel call.
pub static LIBC_CALL: Counter = Counter::new("rawposix::libc_call");
/// Whole `fdtables_syscall` handler (2002): argument checks + `close_virtualfd(-1)` + errno.
pub static FDTABLES_CALL: Counter = Counter::new("rawposix::fdtables_call");
/// Just the `fdtables::close_virtualfd` call made by the 2002 handler. Timed at the call site
/// so the fdtables crate itself needs no changes.
pub static CLOSE_VIRTUALFD: Counter = Counter::new("fdtables::close_virtualfd");

pub static ALL_COUNTERS: &[&Counter] = &[&LIBC_CALL, &FDTABLES_CALL, &CLOSE_VIRTUALFD];
