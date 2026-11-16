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
#[cfg(feature = "lind_perf")]
#[allow(unused_macros)]
macro_rules! perf_span_start {
    ($counter:ident) => {
        crate::perf::$counter.span_start();
    };
}
#[cfg(not(feature = "lind_perf"))]
#[allow(unused_macros)]
macro_rules! perf_span_start {
    ($counter:ident) => {};
}
#[cfg(feature = "lind_perf")]
#[allow(unused_macros)]
macro_rules! perf_span_end {
    ($counter:ident) => {
        crate::perf::$counter.span_end();
    };
}
#[cfg(not(feature = "lind_perf"))]
#[allow(unused_macros)]
macro_rules! perf_span_end {
    ($counter:ident) => {};
}

#[cfg(feature = "lind_perf")]
pub mod perf;

pub mod handler_table;
pub mod threei;
pub mod threei_const;

pub use threei::*;
pub use threei_const::*;
