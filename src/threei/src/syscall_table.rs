use super::threei::RawCallFunc;
use rawposix::fs_calls::{
    close_syscall, mkdir_syscall, open_syscall, read_syscall, mmap_syscall, munmap_syscall,
    brk_syscall, sbrk_syscall, fcntl_syscall, write_syscall, clock_gettime_syscall,
    stat_syscall, fsync_syscall, fdatasync_syscall, rename_syscall, link_syscall,
    unlink_syscall, unlinkat_syscall, readlinkat_syscall, sync_file_range_syscall,
    access_syscall, dup2_syscall, dup3_syscall, futex_syscall, statfs_syscall,
    fstat_syscall, lseek_syscall, pread_syscall, pwrite_syscall, writev_syscall,
    ftruncate_syscall, getdents_syscall, chdir_syscall, fchdir_syscall, rmdir_syscall,
    chmod_syscall, fchmod_syscall, fstatfs_syscall, getcwd_syscall, truncate_syscall, dup_syscall,
    nanosleep_time64_syscall, pipe_syscall, pipe2_syscall, readlink_syscall, mprotect_syscall,
    ioctl_syscall,
};
use rawposix::net_calls::{socket_syscall, connect_syscall, bind_syscall, listen_syscall, 
    accept_syscall, setsockopt_syscall, recv_syscall, recvfrom_syscall,
    sendto_syscall, gethostname_syscall, getsockopt_syscall, getpeername_syscall, socketpair_syscall,
    shutdown_syscall, getsockname_syscall, select_syscall, epoll_create_syscall, epoll_ctl_syscall,
    epoll_wait_syscall, poll_syscall,
};
use rawposix::sys_calls::{
    exec_syscall, exit_syscall, fork_syscall, getpid_syscall, wait_syscall, waitpid_syscall,
    sigaction_syscall,
};

/// According to the Linux version
pub const SYSCALL_TABLE: &[(u64, RawCallFunc)] = &[
    (0, read_syscall),
    (1, write_syscall),
    (2, open_syscall),
    (3, close_syscall),
    (4, stat_syscall),
    (5, fstat_syscall),
    (7, poll_syscall),
    (8, lseek_syscall),
    (9, mmap_syscall),
    (10, mprotect_syscall),
    (11, munmap_syscall),
    (12, brk_syscall),
    (13, sigaction_syscall),
    (16, ioctl_syscall),
    (17, pread_syscall),
    (18, pwrite_syscall),
    (20, writev_syscall),
    (22, pipe_syscall),
    (23, select_syscall),
    (32, dup_syscall),
    (33, dup2_syscall),
    (35, nanosleep_time64_syscall),
    (21, access_syscall),
    (39, getpid_syscall),
    (41, socket_syscall),
    (42, connect_syscall),
    (43, accept_syscall),
    (44, sendto_syscall),
    (45, recvfrom_syscall),
    (47, recv_syscall),
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
    (61, waitpid_syscall),
    (72, fcntl_syscall),
    (74, fsync_syscall),
    (75, fdatasync_syscall),
    (76, truncate_syscall),
    (77, ftruncate_syscall),
    (78, getdents_syscall),
    (79, getcwd_syscall),
    (80, chdir_syscall),
    (81, fchdir_syscall),
    (82, rename_syscall),
    (83, mkdir_syscall),
    (84, rmdir_syscall),
    (86, link_syscall),
    (87, unlink_syscall),
    (89, readlink_syscall),
    (90, chmod_syscall),
    (91, fchmod_syscall),
    (137, statfs_syscall),
    (138, fstatfs_syscall),
    (202, futex_syscall),
    (170, gethostname_syscall),
    (213, epoll_create_syscall),
    (228, clock_gettime_syscall),
    (232, epoll_wait_syscall),
    (233, epoll_ctl_syscall),
    (263, unlinkat_syscall),
    (267, readlinkat_syscall),
    (277, sync_file_range_syscall),
    (292, dup3_syscall),
    (293, pipe2_syscall),
    (1004, sbrk_syscall),
];

// //! rawposix syscall dispatcher table
// //! Source of truth for syscall numbers: Linux x86_64 syscall table
// //! https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl
// //! Keep these in sync with glibc's lind_syscall_num.h
// use super::threei::RawCallFunc;
// use rawposix::fs_calls::{
//     brk_syscall, clock_gettime_syscall, close_syscall, dup2_syscall, dup_syscall, fcntl_syscall,
//     futex_syscall, lseek_syscall, mkdir_syscall, mmap_syscall, munmap_syscall,
//     nanosleep_time64_syscall, open_syscall, pipe2_syscall, pipe_syscall, read_syscall,
//     sbrk_syscall, unlink_syscall, write_syscall,
// };
// use rawposix::sys_calls::{
//     exec_syscall, exit_syscall, fork_syscall, getpid_syscall, wait_syscall, waitpid_syscall,
// };

// /// According to the Linux version
// /// In glibc, waitpid() is actually implemented by calling wait4(), 
// /// so the Linux kernel itself does not provide a separate syscall 
// /// number for waitpid.
// /// In lind-wasm, however, we treat wait and waitpid as distinct 
// /// syscalls, assigning them arbitrary syscall numbers. These are 
// /// only resolved later in rawposix, where wait is internally implemented 
// /// by invoking waitpid with options = 0.
// /// This design choice may become a future TODO: we could adopt a 
// /// similar approach in lind-glibc by having wait() directly call 
// /// waitpid(), and then remove the separate wait implementation from 
// /// rawposix.
// pub const SYSCALL_TABLE: &[(u64, RawCallFunc)] = &[
//     (0, read_syscall),
//     (1, write_syscall),
//     (2, open_syscall),
//     (3, close_syscall),
//     (8, lseek_syscall),
//     (9, mmap_syscall),
//     (11, munmap_syscall),
//     (12, brk_syscall),
//     (22, pipe_syscall),
//     (32, dup_syscall),
//     (33, dup2_syscall),
//     (35, nanosleep_time64_syscall),
//     (39, getpid_syscall),
//     (57, fork_syscall),
//     (59, exec_syscall),
//     (60, exit_syscall),
//     (61, wait_syscall),
//     (72, fcntl_syscall),
//     (83, mkdir_syscall),
//     (87, unlink_syscall),
//     (202, futex_syscall),
//     (228, clock_gettime_syscall),
//     (293, pipe2_syscall),
//     (400, waitpid_syscall),
//     (1004, sbrk_syscall),
// ];
