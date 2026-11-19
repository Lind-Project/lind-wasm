//! lind-perf: per-layer timing of the lind-wasm syscall path.
//!
//! This crate is an *optional* dependency of every crate it instruments. It is only compiled
//! and linked when lind-boot is built with `--features lind_perf` (`make lind-boot-perf`).
//! In a normal build the instrumented crates don't depend on it at all, and their
//! `perf_timer!` call sites expand to nothing.
//!
//! See `README.md` for how measurements are taken and reported.

mod bench;
mod counter;
mod names;
mod report;
mod timers;

pub use bench::{CallGuard, Origin, PerfScope, SLOTS, perf_scope, set_perf_scope, timing_active};
pub use counter::{Counter, Scope, Unit};
pub use names::syscall_name;
pub use report::{Derived, print_derived, print_report, print_syscall_table};
pub use timers::{TimerKind, calibrate_overhead, set_timer_kind, timer_kind};
