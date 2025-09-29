//! rawposix syscall dispatcher table
//! Source of truth for syscall numbers: Linux x86_64 syscall table
//! https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl
//! https://filippo.io/linux-syscall-table/ 
//! Keep these in sync with glibc's lind_syscall_num.h
use super::threei::RawCallFunc;
use rawposix::fs_calls::{
    brk_syscall, clock_gettime_syscall, close_syscall, dup2_syscall, dup_syscall, fcntl_syscall,
    futex_syscall, lseek_syscall, mkdir_syscall, mmap_syscall, munmap_syscall,
    nanosleep_time64_syscall, open_syscall, pipe2_syscall, pipe_syscall, read_syscall,
    sbrk_syscall, unlink_syscall, write_syscall,
};
use rawposix::net_calls::{socket_syscall, connect_syscall, bind_syscall, listen_syscall, 
    accept_syscall, setsockopt_syscall, recvfrom_syscall, sendto_syscall, gethostname_syscall, 
    getsockopt_syscall, getpeername_syscall, socketpair_syscall, shutdown_syscall, getsockname_syscall, 
};
use rawposix::sys_calls::{
    exec_syscall, exit_syscall, fork_syscall, getpid_syscall, wait_syscall, waitpid_syscall,
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
    (1, write_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (8, lseek_syscall),
    (9, mmap_syscall),
    (11, munmap_syscall),
    (12, brk_syscall),
    (22, pipe_syscall),
    (32, dup_syscall),
    (33, dup2_syscall),
    (35, nanosleep_time64_syscall),
    (39, getpid_syscall),
    (41, socket_syscall),
    (42, connect_syscall),
    (43, accept_syscall),
    (44, sendto_syscall),
    (45, recvfrom_syscall),
    (48, shutdown_syscall),
    (49, bind_syscall),
    (50, listen_syscall),
    (51, getsockname_syscall),
    (52, getpeername_syscall),
    (53, socketpair_syscall),
    (54, setsockopt_syscall),
    (55, getsockopt_syscall),
    (57, fork_syscall),
    (59, exec_syscall),
    (60, exit_syscall),
    (61, wait_syscall),
    (72, fcntl_syscall),
    (83, mkdir_syscall),
    (87, unlink_syscall),
    (202, futex_syscall),
    (228, clock_gettime_syscall),
    (293, pipe2_syscall),
    (400, waitpid_syscall),
    (1004, sbrk_syscall),
];
