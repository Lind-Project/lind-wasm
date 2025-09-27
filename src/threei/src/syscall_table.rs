//! rawposix syscall dispatcher table
//! Source of truth for syscall numbers: Linux x86_64 syscall table
//! https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl
//! https://filippo.io/linux-syscall-table/ 
//! Keep these in sync with glibc's lind_syscall_num.h
use super::threei::RawCallFunc;
use rawposix::fs_calls::{
    brk_syscall, close_syscall, fcntl_syscall, ioctl_syscall, mkdir_syscall, mmap_syscall, 
    munmap_syscall, open_syscall, read_syscall, sbrk_syscall, unlink_syscall,
};
use rawposix::net_calls::{socket_syscall, connect_syscall, bind_syscall, listen_syscall, 
    accept_syscall, setsockopt_syscall, send_syscall, recv_syscall, recvfrom_syscall, sendto_syscall, gethostname_syscall, 
    getsockopt_syscall, getpeername_syscall, socketpair_syscall, poll_syscall, select_syscall,
    epoll_create_syscall, epoll_ctl_syscall, epoll_wait_syscall,
};

/// According to the Linux version
/// In glibc, waitpid() is actually implemented by calling wait4(), 
/// so the Linux kernel itself does not provide a separate syscall 
/// number for waitpid.
/// In lind-wasm, however, we treat wait and waitpid as distinct 
/// syscalls, assigning them arbitrary syscall numbers. These are 
/// only resolved later in rawposix, where wait is internally implemented 
/// by invoking waitpid with options = 0.
/// This design choice may become a future TODO: we could adopt a 
/// similar approach in lind-glibc by having wait() directly call 
/// waitpid(), and then remove the separate wait implementation from 
/// rawposix.
pub const SYSCALL_TABLE: &[(u64, RawCallFunc)] = &[
    (0, read_syscall),
    // (1, write_syscall), // TODO: Implement write_syscall in rawposix
    (2, open_syscall),
    (3, close_syscall),
    // (8, lseek_syscall), // TODO: Implement lseek_syscall in rawposix
    (9, mmap_syscall),
    (11, munmap_syscall),
    (12, brk_syscall),
    // (22, pipe_syscall), // TODO: Implement pipe_syscall in rawposix
    // (32, dup_syscall), // TODO: Implement dup_syscall in rawposix
    // (33, dup2_syscall), // TODO: Implement dup2_syscall in rawposix
    // (35, nanosleep_time64_syscall), // TODO: Implement nanosleep_time64_syscall in rawposix
    // (39, getpid_syscall), // TODO: Implement getpid_syscall in rawposix
    (41, socket_syscall),
    (42, connect_syscall),
    (43, accept_syscall),
    (46, send_syscall),
    // (48, shutdown_syscall), // TODO: Implement shutdown_syscall in rawposix
    (49, bind_syscall),
    (50, listen_syscall),
    // (51, getsockname_syscall), // TODO: Implement getsockname_syscall in rawposix
    (54, setsockopt_syscall),
    (44, sendto_syscall),
    (45, recvfrom_syscall),
    (47, recv_syscall),
    (52, getpeername_syscall),
    (53, socketpair_syscall),
    (55, getsockopt_syscall),
    // (57, fork_syscall), // TODO: Implement fork_syscall in rawposix
    // (59, exec_syscall), // TODO: Implement exec_syscall in rawposix
    // (60, exit_syscall), // TODO: Implement exit_syscall in rawposix
    // (61, wait_syscall), // TODO: Implement wait_syscall in rawposix
    (16, ioctl_syscall),        // ioctl
    (72, fcntl_syscall),
    (83, mkdir_syscall),
    (87, unlink_syscall),
    // (202, futex_syscall), // TODO: Implement futex_syscall in rawposix
    // (228, clock_gettime_syscall), // TODO: Implement clock_gettime_syscall in rawposix
    // (293, pipe2_syscall), // TODO: Implement pipe2_syscall in rawposix
    // (400, waitpid_syscall), // TODO: Implement waitpid_syscall in rawposix
    (1004, sbrk_syscall),
    // epoll/poll/select syscalls
    (7, poll_syscall),           // poll
    (23, select_syscall),        // select  
    (213, epoll_create_syscall), // epoll_create
    (233, epoll_ctl_syscall),    // epoll_ctl
    (232, epoll_wait_syscall),   // epoll_wait
];
