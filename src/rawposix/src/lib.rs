// RawPOSIX Library - Core POSIX system call implementations for GrateOS-WASM
//
// This library provides POSIX-compliant system call implementations that operate
// within the GrateOS-WASM sandbox environment using the 3i (Three Interposition) system.

pub mod fs_calls;
pub mod init;
pub mod net_calls;
pub mod sys_calls;
pub mod syscall_table;

pub use syscall_table::*;
