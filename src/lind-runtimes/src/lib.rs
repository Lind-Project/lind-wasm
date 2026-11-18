//! Scaffolding for Lind's runtime implementations of the draft Lind-IF.
//!
//! The active Wasmtime and MPK backends still live in lind-boot. These
//! implementations explicitly return ENOSYS until their mechanisms are moved.

pub mod lind_mpk;
pub mod lind_wasm;

use typemap::lind_if::{Errno, LindError, LindResult};

fn unsupported<T>() -> LindResult<T> {
    Err(LindError::Other(Errno::ENOSYS))
}
