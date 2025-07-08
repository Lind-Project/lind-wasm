use rawposix::fs_calls::{
    mkdir_syscall, open_syscall, close_syscall, read_syscall, 
};
pub use rawposix::fs_calls::mmap_syscall;
use rawposix::sys_calls::exit_syscall;
use super::threei::Raw_CallFunc;

/// According to the Linux standard
pub const SYSCALL_TABLE: &[(u64, Raw_CallFunc)] = &[
    (0, read_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (21, mmap_syscall),
    (30, exit_syscall),
    (83, mkdir_syscall),
];
