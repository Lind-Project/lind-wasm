use rawposix::fs_calls::{
    brk_syscall, clock_gettime_syscall, close_syscall, dup2_syscall, fcntl_syscall,
    mkdir_syscall, munmap_syscall, nanosleep_time64_syscall, open_syscall, dup_syscall, 
    pipe2_syscall, pipe_syscall, sbrk_syscall, write_syscall, futex_syscall, read_syscall,

};
pub use rawposix::fs_calls::mmap_syscall;
use rawposix::sys_calls::{
    exec_syscall, exit_syscall, fork_syscall, getpid_syscall, wait_syscall, waitpid_syscall
};
use rawposix::net_calls::{socket_syscall,accept_syscall,bind_syscall,connect_syscall,listen_syscall,
    setsockopt_syscall,send_syscall,recv_syscall, sendto_syscall, recvfrom_syscall, gethostname_syscall, 
    getsockopt_syscall, getpeername_syscall, poll_syscall, epoll_create_syscall, 
    epoll_ctl_syscall, epoll_wait_syscall, socketpair_syscall};


use super::threei::Raw_CallFunc;

/// According to the Linux standard
pub const SYSCALL_TABLE: &[(u64, Raw_CallFunc)] = &[
    (0, read_syscall),
    (1, write_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (7, poll_syscall),
    (10, open_syscall),
    (11, close_syscall),
    (13, write_syscall),
    (21, mmap_syscall),
    (22, munmap_syscall),
    (24, dup_syscall),
    (25, dup2_syscall),
    (28, fcntl_syscall),
    (30, exit_syscall),
    (31, getpid_syscall),
    (32, dup_syscall),
    (41, socket_syscall),
    (42, connect_syscall),
    (43, accept_syscall),
    (44, sendto_syscall),
    (45, recvfrom_syscall),
    (46, send_syscall),
    (47, recv_syscall),
    (49, bind_syscall),
    (50, listen_syscall),
    (52, getpeername_syscall),
    (53, socketpair_syscall),
    (54, setsockopt_syscall),
    (55, getsockopt_syscall),
    (66, pipe_syscall),
    (67, pipe2_syscall),
    (68, fork_syscall),
    (69, exec_syscall),
    (83, mkdir_syscall),
    (98, futex_syscall),
    (131, mkdir_syscall),
    (170, gethostname_syscall),
    (172, wait_syscall),
    (173, waitpid_syscall),
    (175, brk_syscall),
    (176, sbrk_syscall),
    (181, nanosleep_time64_syscall),
    (191, clock_gettime_syscall),
    (213, epoll_create_syscall),
    (232, epoll_wait_syscall),
    (233, epoll_ctl_syscall),
];
