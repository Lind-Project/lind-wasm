
//! rawposix syscall dispatcher table
//! Source of truth for syscall numbers: Linux x86_64 syscall table
//! https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl
//! https://filippo.io/linux-syscall-table/ 
//! Keep these in sync with glibc's lind_syscall_num.h
use super::threei::RawCallFunc;
use rawposix::fs_calls::{
    close_syscall, mkdir_syscall, open_syscall, read_syscall, mmap_syscall, munmap_syscall,
    brk_syscall, sbrk_syscall, fcntl_syscall, write_syscall, clock_gettime_syscall,
    fstat_syscall, stat_syscall, lseek_syscall, pread_syscall, pwrite_syscall, writev_syscall,
    ftruncate_syscall, getdents_syscall, chdir_syscall, fchdir_syscall, rmdir_syscall,
    chmod_syscall, fchmod_syscall, fstatfs_syscall, getcwd_syscall, truncate_syscall, dup_syscall,
    dup2_syscall, dup3_syscall, futex_syscall,
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
    (4, stat_syscall),
    (5, fstat_syscall),
    (8, lseek_syscall),
    (9, mmap_syscall),
    (11, munmap_syscall),
    (12, brk_syscall),
    (17, pread_syscall),
    (18, pwrite_syscall),
    (20, writev_syscall),
    (32, dup_syscall),
    (35, nanosleep_time64_syscall),
    (39, getpid_syscall),
    (41, dup2_syscall),
    (57, fork_syscall),
    (59, exec_syscall),
    (60, exit_syscall),
    (61, wait_syscall),
    (72, fcntl_syscall),
    (76, truncate_syscall),
    (77, ftruncate_syscall),
    (78, getdents_syscall),
    (79, getcwd_syscall),
    (80, chdir_syscall),
    (81, fchdir_syscall),
    (83, mkdir_syscall),
    (84, rmdir_syscall),
    (90, chmod_syscall),
    (91, fchmod_syscall),
    (138, fstatfs_syscall),
    (202, futex_syscall),
    (228, clock_gettime_syscall),
    (292, dup3_syscall),
    (293, pipe2_syscall),
    (400, waitpid_syscall),
    (1004, sbrk_syscall),
];
