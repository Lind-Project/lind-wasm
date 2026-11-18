//! Runtime implementations of the draft Lind-IF.
//!
//! Memory operations are active; lifecycle operations remain in lind-boot.

pub mod lind_mpk;
pub mod lind_wasm;
mod memory;

use typemap::lind_if::{Errno, LindError, LindResult};

fn unsupported<T>() -> LindResult<T> {
    Err(LindError::Other(Errno::ENOSYS))
}
