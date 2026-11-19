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

// Steps inside real syscall handlers. These record on every call of their syscall while timing is
// on, so they are meant for `--perf-scope=all` runs of real programs (e.g. lmbench), not for the
// benchmark syscalls. They are listed separately so the benchmark runs don't select them.

/// `read_syscall`: the whole handler.
pub static READ: Counter = Counter::new("rawposix::read");
/// `read_syscall`: `convert_fd_to_host`, the virtual fd looked up in fdtables.
pub static READ_FD_LOOKUP: Counter = Counter::new("rawposix::read::fd_lookup");
/// `read_syscall`: the kernel `read`.
pub static READ_KERNEL: Counter = Counter::new("rawposix::read::kernel");
/// `write_syscall`: the whole handler.
pub static WRITE: Counter = Counter::new("rawposix::write");
/// `write_syscall`: `convert_fd_to_host`.
pub static WRITE_FD_LOOKUP: Counter = Counter::new("rawposix::write::fd_lookup");
/// `write_syscall`: the kernel `write`.
pub static WRITE_KERNEL: Counter = Counter::new("rawposix::write::kernel");
/// `stat_syscall`: the whole handler.
pub static STAT: Counter = Counter::new("rawposix::stat");
/// `stat_syscall`: `sc_convert_path_to_host`, the cage's path made into a host path.
pub static STAT_PATH: Counter = Counter::new("rawposix::stat::path");
/// `stat_syscall`: the kernel `stat`.
pub static STAT_KERNEL: Counter = Counter::new("rawposix::stat::kernel");
/// `stat_syscall`: the result converted and copied into the cage's `stat` struct.
pub static STAT_COPY_OUT: Counter = Counter::new("rawposix::stat::copy_out");
/// `select_syscall` with a zero timeout (a poll, as in lmbench's lat_select), from the timeout
/// argument on. Calls with a non-zero timeout are left out: they include sleeping.
pub static SELECT: Counter = Counter::new("rawposix::select");
/// `select_syscall`: `fdtables::prepare_bitmasks_for_select`, the cage's fd sets turned into
/// kernel fd sets.
pub static SELECT_PREPARE: Counter = Counter::new("rawposix::select::prepare_fdsets");
/// `select_syscall`: the loop around the kernel `select` (timeout chunks, fd set copies).
pub static SELECT_KERNEL_LOOP: Counter = Counter::new("rawposix::select::kernel_loop");
/// `select_syscall`: the kernel results turned back into the cage's fd sets and copied out.
pub static SELECT_RESULTS: Counter = Counter::new("rawposix::select::results");
/// `clock_gettime_syscall`: the whole handler.
pub static CLOCK_GETTIME: Counter = Counter::new("rawposix::clock_gettime");
/// `clock_gettime_syscall`: the raw `syscall(SYS_clock_gettime)` (a real kernel entry, not the
/// vDSO that glibc uses natively).
pub static CLOCK_GETTIME_KERNEL: Counter = Counter::new("rawposix::clock_gettime::kernel");

pub static HANDLER_COUNTERS: &[&Counter] = &[
    &READ,
    &READ_FD_LOOKUP,
    &READ_KERNEL,
    &WRITE,
    &WRITE_FD_LOOKUP,
    &WRITE_KERNEL,
    &STAT,
    &STAT_PATH,
    &STAT_KERNEL,
    &STAT_COPY_OUT,
    &SELECT,
    &SELECT_PREPARE,
    &SELECT_KERNEL_LOOP,
    &SELECT_RESULTS,
    &CLOCK_GETTIME,
    &CLOCK_GETTIME_KERNEL,
];
