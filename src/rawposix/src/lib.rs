//! This module contains actual syscall implementation in RawPOSIX
pub mod fs_calls;
pub mod sys_calls;

pub use sys_calls::{lindrustfinalize, lindrustinit};
pub use fs_calls::{shmget_syscall, shmat_syscall, shmdt_syscall, shmctl_syscall};
