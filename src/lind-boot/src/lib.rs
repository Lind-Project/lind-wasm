pub mod cli;
pub mod lind_wasmtime;

pub use cli::CliOptions;
pub use lind_wasmtime::{execute_wasmtime, precompile_module};
