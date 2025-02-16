use rawposix::syscalls::fs_calls::{
    brk_syscall, mkdir_syscall, mmap_syscall, munmap_syscall, open_syscall, dup_syscall,
    sbrk_syscall, write_syscall, clock_gettime_syscall, fcntl_syscall, dup2_syscall,
};
use rawposix::syscalls::sys_calls::{exec_syscall, exit_syscall, fork_syscall};
use super::threei::CallFunc;

/// Will replace syscall number with Linux Standard after confirming the refactoring details
pub const SYSCALL_TABLE: &[(u64, CallFunc)] = &[
    (13, write_syscall),
    (10, open_syscall),
    (21, mmap_syscall),
    (22, munmap_syscall),
    (24, dup_syscall),
    (25, dup2_syscall),
    (28, fcntl_syscall),
    (30, exit_syscall),
    (69, exec_syscall),
    (83, mkdir_syscall),
    (171, fork_syscall),
    (175, brk_syscall),
    (176, sbrk_syscall),
    (191, clock_gettime_syscall),
];
