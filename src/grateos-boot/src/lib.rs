pub mod cli;
pub mod grateos_wasmtime;

pub use cli::CliOptions;
pub use grateos_wasmtime::{execute_wasmtime, precompile_module};
