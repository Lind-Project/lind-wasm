use rawposix::syscalls::fs_calls::{
    mkdir_syscall, open_syscall, close_syscall, read_syscall,
};
pub use rawposix::syscalls::fs_calls::mmap_syscall;
use super::threei::RawCallFunc;

/// According to the Linux standard
pub const SYSCALL_TABLE: &[(u64, RawCallFunc)] = &[
    (0, read_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (21, mmap_syscall),
    (83, mkdir_syscall),
];
