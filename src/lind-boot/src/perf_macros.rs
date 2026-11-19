// lind-perf timing hooks. With the `lind_perf` feature they start/stop a timer on a counter
// from this crate's `perf` module; without it they expand to nothing, so normal builds contain
// no perf code. Included with `#[macro_use]` before the other modules in both main.rs and
// lib.rs, since both compile `lind_wasmtime`.
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
