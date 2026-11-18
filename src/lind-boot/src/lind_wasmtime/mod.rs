pub mod execute;
pub mod host;
mod runtime;
pub mod trampoline;

pub use execute::{execute_wasmtime, precompile_module};
