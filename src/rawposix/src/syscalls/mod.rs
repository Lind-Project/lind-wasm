//! This module contains actual syscall implementation in RawPOSIX
pub mod fs_calls;
pub mod net_calls;
pub mod sys_calls;

pub use sys_calls::{lindrustfinalize, lindrustinit};
