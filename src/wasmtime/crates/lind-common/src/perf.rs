//! lind-perf counters and perf-only syscalls for lind-common (perf builds only).
use lind_perf::{Counter, Unit};

/// Ended in the `clone` rewind path of the syscall entry (see lind-multi-process `perf.rs`).
pub use wasmtime_lind_multi_process::perf::SPAN_FORK_PARENT_REWIND;

/// Benchmark syscall numbers live in 2001..=2999. Only these are timed.
pub const BENCH_SYSCALL_MIN: u32 = 2001;
pub const BENCH_SYSCALL_MAX: u32 = 2999;

/// 2003: returns immediately at the syscall entry, before 3i. The guest's own timing of this
/// call is the pure Wasm -> host -> Wasm round trip.
pub const NULL_SYSCALL: u32 = 2003;
/// 2004: the guest reports its loop timing: arg1 = total nanoseconds, arg2 = iterations.
pub const GUEST_REPORT_SYSCALL: u32 = 2004;

/// The guest's own measurement: loop time / iterations, as seen from inside the Wasm program.
pub static GUEST_PER_CALL: Counter = Counter::with_unit("guest::per_call", Unit::Nanos);
/// Whole `make-syscall` host function (the unified syscall entry from guest glibc).
pub static SYSCALL_ENTRY: Counter = Counter::new("lind_common::syscall_entry");

pub static ALL_COUNTERS: &[&Counter] = &[&GUEST_PER_CALL, &SYSCALL_ENTRY];

/// Should this syscall be timed? Benchmark syscalls always; every syscall with
/// `--perf-scope=all`. The guest-report call is bookkeeping and never timed.
#[inline(always)]
pub fn should_time(call_number: u32) -> bool {
    call_number != GUEST_REPORT_SYSCALL
        && (lind_perf::perf_scope() == lind_perf::PerfScope::All
            || (BENCH_SYSCALL_MIN..=BENCH_SYSCALL_MAX).contains(&call_number))
}

/// Handle the perf-only syscalls that never reach 3i. Returns `None` for everything else.
#[inline(always)]
pub fn handle_perf_syscall(call_number: u32, arg1: u64, arg2: u64) -> Option<i64> {
    match call_number {
        NULL_SYSCALL => Some(0),
        GUEST_REPORT_SYSCALL => {
            GUEST_PER_CALL.add(arg1, arg2);
            Some(0)
        }
        _ => None,
    }
}
