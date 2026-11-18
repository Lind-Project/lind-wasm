//! Runtime implementations of the draft Lind-IF.
//!
//! Memory operations and signal notification are active; lifecycle operations
//! remain in lind-boot.

pub mod lind_mpk;
pub mod lind_wasm;
mod memory;
mod signal;

use lind_if::{Errno, LindError, LindResult};

fn unsupported<T>() -> LindResult<T> {
    Err(LindError::Other(Errno::ENOSYS))
}
