// lind-perf timing macros (expand to nothing without the `lind_perf` feature).
#[macro_use]
mod perf_macros;
#[cfg(feature = "lind_perf")]
pub mod perf;

pub mod cli;
pub mod lind_wasmtime;

pub use cli::CliOptions;
pub use lind_wasmtime::{execute_wasmtime, precompile_module};
