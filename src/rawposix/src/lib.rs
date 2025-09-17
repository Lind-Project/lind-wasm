// RawPOSIX Library - Core POSIX system call implementations for Lind-WASM
//
// This library provides POSIX-compliant system call implementations that operate
// within the Lind-WASM sandbox environment using the 3i (Three Interposition) system.

pub mod fs_calls;
pub mod sys_calls;

// Re-export key syscalls for testing and integration
pub use fs_calls::{poll_syscall, select_syscall, open_syscall, read_syscall, close_syscall, mkdir_syscall};
