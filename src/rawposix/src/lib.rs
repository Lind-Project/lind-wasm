// RawPOSIX Library - Core POSIX system call implementations for Lind-WASM
//
// This library provides POSIX-compliant system call implementations that operate
// within the Lind-WASM sandbox environment using the 3i (Three Interposition) system.

// lind-perf timing hooks. With the `lind_perf` feature they start/stop a timer on a counter
// from this crate's `perf` module; without it they expand to nothing, so normal builds contain
// no perf code. Defined before the `mod` declarations so every module can use them.
#[cfg(feature = "lind_perf")]
#[allow(unused_macros)]
macro_rules! perf_timer {
    ($guard:ident = $counter:ident) => {
        let $guard = crate::perf::$counter.timer();
    };
}
#[cfg(not(feature = "lind_perf"))]
#[allow(unused_macros)]
macro_rules! perf_timer {
    ($guard:ident = $counter:ident) => {};
}
#[cfg(feature = "lind_perf")]
#[allow(unused_macros)]
macro_rules! perf_stop {
    ($guard:ident) => {
        drop($guard);
    };
}
#[cfg(not(feature = "lind_perf"))]
#[allow(unused_macros)]
macro_rules! perf_stop {
    ($guard:ident) => {};
}
// Like `perf_timer!`, but only times when `$cond` is true. Without the feature the condition is
// not compiled either.
#[cfg(feature = "lind_perf")]
#[allow(unused_macros)]
macro_rules! perf_timer_if {
    ($guard:ident = $counter:ident, $cond:expr) => {
        let $guard = if $cond {
            crate::perf::$counter.timer()
        } else {
            None
        };
    };
}
#[cfg(not(feature = "lind_perf"))]
#[allow(unused_macros)]
macro_rules! perf_timer_if {
    ($guard:ident = $counter:ident, $cond:expr) => {};
}

#[cfg(feature = "lind_perf")]
pub mod bench_calls;
#[cfg(feature = "lind_perf")]
pub mod perf;

pub mod fs_calls;
pub mod init;
pub mod net_calls;
pub mod sys_calls;
pub mod syscall_table;

pub use syscall_table::*;
