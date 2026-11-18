//! Runtime implementations of the draft Lind-IF.
//!
//! Memory operations and signal notification are active. Clone, exec, and exit
//! are handled through `lifecycle::SyscallRuntime`, implemented by the MPK
//! runtime (`lind_mpk`) and by the Wasm runtime on top of Wasmtime's
//! multi-process support (`lind_wasm::lifecycle::WasmLifecycle`).

pub mod lifecycle;
pub mod lind_mpk;
pub mod lind_wasm;
mod memory;
mod signal;

use lind_if::{Errno, LindError, LindResult};

fn unsupported<T>() -> LindResult<T> {
    Err(LindError::Other(Errno::ENOSYS))
}
