// lind-perf timing hooks. With the `lind_perf` feature they start/stop a timer on a counter
// from this crate's `perf` module; without it they expand to nothing, so normal builds contain
// no perf code. The `_any` forms record whenever the counter is the one enabled for the run, not
// only during a timed benchmark syscall: fork and exit run outside one, partly on new threads.
#[cfg(feature = "lind_perf")]
#[allow(unused_macros)]
macro_rules! perf_timer_any {
    ($guard:ident = $counter:ident) => {
        let $guard = crate::perf::$counter.timer_any();
    };
}
#[cfg(not(feature = "lind_perf"))]
#[allow(unused_macros)]
macro_rules! perf_timer_any {
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

#[cfg(feature = "lind_perf")]
pub mod perf;

pub mod cage;
pub mod memory;
pub mod signal;

pub use cage::*;
pub use memory::*;
pub use signal::*;
