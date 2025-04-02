use super::threei::CallFunc;
use rawposix::syscalls::fs_calls::{
    brk_syscall, clock_gettime_syscall, dup2_syscall, dup_syscall, fcntl_syscall, mkdir_syscall,
    mmap_syscall, munmap_syscall, open_syscall, sbrk_syscall, write_syscall, futex_syscall
};
use rawposix::syscalls::sys_calls::{exec_syscall, exit_syscall, fork_syscall};
use rawposix::syscalls::net_calls::{socket_syscall,accept_syscall,bind_syscall,connect_syscall,listen_syscall,setsockopt_syscall,send_syscall,recv_syscall};

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
    (33, bind_syscall),
    (34, send_syscall),
    (36, recv_syscall),
    (38, connect_syscall),
    (39, listen_syscall),
    (40, accept_syscall),
    (44, setsockopt_syscall),
    (69, exec_syscall),
    (83, mkdir_syscall),
    (98, futex_syscall),
    (136, socket_syscall),
    (171, fork_syscall),
    (175, brk_syscall),
    (176, sbrk_syscall),
    (191, clock_gettime_syscall),
];
