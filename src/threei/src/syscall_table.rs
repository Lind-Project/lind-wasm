use rawposix::fs_calls::{
    brk_syscall, chdir_syscall, chmod_syscall, clock_gettime_syscall, close_syscall, dup2_syscall, fcntl_syscall,
    mkdir_syscall, munmap_syscall, nanosleep_time64_syscall, open_syscall, dup_syscall, 
    pipe2_syscall, pipe_syscall, rmdir_syscall, sbrk_syscall, write_syscall, futex_syscall, read_syscall,
    pread_syscall, pwrite_syscall, fstat_syscall,

};
pub use rawposix::fs_calls::mmap_syscall;
use rawposix::sys_calls::{
    exec_syscall, exit_syscall, fork_syscall, getpid_syscall, wait_syscall, waitpid_syscall
};
use super::threei::Raw_CallFunc;

/// According to the Linux standard
pub const SYSCALL_TABLE: &[(u64, Raw_CallFunc)] = &[
    (0, read_syscall),
    (1, write_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (5, fstat_syscall),
    (10, open_syscall),
    (11, close_syscall),
    (12, chdir_syscall),
    (13, write_syscall),
    (17, pread_syscall),
    (18, pwrite_syscall),
    (21, mmap_syscall),
    (22, munmap_syscall),
    (24, dup_syscall),
    (25, dup2_syscall),
    (28, fcntl_syscall),
    (30, exit_syscall),
    (31, getpid_syscall),
    (32, dup_syscall),
    (66, pipe_syscall),
    (67, pipe2_syscall),
    (68, fork_syscall),
    (69, exec_syscall),
    (83, mkdir_syscall),
    (84, rmdir_syscall),
    (90, chmod_syscall),
    (98, futex_syscall),
    (131, mkdir_syscall),
    (172, wait_syscall),
    (173, waitpid_syscall),
    (175, brk_syscall),
    (176, sbrk_syscall),
    (181, nanosleep_time64_syscall),
    (191, clock_gettime_syscall),
];
