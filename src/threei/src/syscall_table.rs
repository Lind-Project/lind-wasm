use rawposix::fs_calls::{
    brk_syscall, clock_gettime_syscall, close_syscall, dup2_syscall, fcntl_syscall,
    mkdir_syscall, munmap_syscall, nanosleep_time64_syscall, open_syscall, dup_syscall, 
    pipe2_syscall, pipe_syscall, sbrk_syscall, write_syscall, futex_syscall, read_syscall,
    mmap_syscall, lseek_syscall, unlink_syscall
};
use rawposix::sys_calls::{
    exec_syscall, exit_syscall, fork_syscall, getpid_syscall, wait_syscall, waitpid_syscall
};
use super::threei::Raw_CallFunc;

/// According to the Linux version
pub const SYSCALL_TABLE: &[(u64, Raw_CallFunc)] = &[
    (0, read_syscall),
    (1, write_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (8, lseek_syscall),
    (9, mmap_syscall),
    (10, open_syscall),
    (11, munmap_syscall),
    (12, brk_syscall),
    (22, pipe_syscall),
    (32, dup_syscall),
    (33, dup2_syscall),
    (35, nanosleep_time64_syscall),
    (39, getpid_syscall),
    (57, fork_syscall),
    (60, exit_syscall),
    (61, wait_syscall),
    (61, waitpid_syscall),
    (69, exec_syscall),
    (72, fcntl_syscall),
    (83, mkdir_syscall),
    (87, unlink_syscall),
    (202, futex_syscall),
    (228, clock_gettime_syscall),
    (293, pipe2_syscall),
    (1004, sbrk_syscall),
];
