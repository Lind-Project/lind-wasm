//! Runtime implementations of the draft Lind-IF.
//!
//! Memory operations, signal notification, clone (`duplicate_cage` and
//! `duplicate_thread`), exec (`init_cage` staging, `create_thread`,
//! `apply_staging_cage`) and exit (`terminate_thread`, `terminate_cage`) are
//! active for the MPK runtime (`lind_mpk`) and the Wasm runtime built on
//! Wasmtime's multi-process support (`lind_wasm`).

mod staging;
pub mod lind_mpk;
pub mod lind_wasm;
mod memory;
mod signal;

use lind_if::{Errno, LindError, LindResult};

pub fn unsupported<T>() -> LindResult<T> {
    Err(LindError::Other(Errno::ENOSYS))
}
