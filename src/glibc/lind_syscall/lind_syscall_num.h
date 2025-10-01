/*
 * Lind syscall number definitions
 *
 * This file contains syscall number constants used by Lind WASM to map
 * between glibc function calls and the corresponding system calls.
 *
 * Source of truth: Linux x86_64 syscall table. Keep these values aligned with:
 *   https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl
 * (Also see the historical summary: https://filippo.io/linux-syscall-table/)
 */

#ifndef _LIND_SYSCALL_NUM_H
#define _LIND_SYSCALL_NUM_H
 
#define READ_SYSCALL 0
#define WRITE_SYSCALL 1
#define OPEN_SYSCALL 2
#define CLOSE_SYSCALL 3
#define XSTAT_SYSCALL 4
#define FXSTAT_SYSCALL 5

#define POLL_SYSCALL 7
#define LSEEK_SYSCALL 8
#define MMAP_SYSCALL 9
#define MPROTECT_SYSCALL 10
#define MUNMAP_SYSCALL 11
#define BRK_SYSCALL 12
#define SIGACTION_SYSCALL 13
#define SIGPROCMASK_SYSCALL 14

#define IOCTL_SYSCALL 16
#define PREAD_SYSCALL 17
#define PWRITE_SYSCALL 18

#define WRITEV_SYSCALL 20
#define ACCESS_SYSCALL 21
#define PIPE_SYSCALL 22
#define SELECT_SYSCALL 23

#define SHMGET_SYSCALL 29
#define SHMAT_SYSCALL 30
#define SHMCTL_SYSCALL 31
#define DUP_SYSCALL 32
#define DUP2_SYSCALL 33

#define NANOSLEEP_TIME64_SYSCALL 35

#define SETITIMER_SYSCALL 38
#define GETPID_SYSCALL 39

#define SOCKET_SYSCALL 41
#define CONNECT_SYSCALL 42
#define ACCEPT_SYSCALL 43
#define SENDTO_SYSCALL 44
#define RECVFROM_SYSCALL 45
#define SENDMSG_SYSCALL 46
#define RECVMSG_SYSCALL 47
#define SHUTDOWN_SYSCALL 48
#define BIND_SYSCALL 49
#define LISTEN_SYSCALL 50
#define GETSOCKNAME_SYSCALL 51
#define GETPEERNAME_SYSCALL 52
#define SOCKETPAIR_SYSCALL 53
#define SETSOCKOPT_SYSCALL 54
#define GETSOCKOPT_SYSCALL 55
#define CLONE_SYSCALL 56
#define FORK_SYSCALL 57
#define EXEC_SYSCALL 59
#define EXECVE_SYSCALL 59
#define EXIT_SYSCALL 60
#define WAIT_SYSCALL 61
#define WAITPID_SYSCALL 61
#define KILL_SYSCALL 62

#define SHMDT_SYSCALL 67

#define FCNTL_SYSCALL 72
#define FLOCK_SYSCALL 73
#define FSYNC_SYSCALL 74
#define FDATASYNC_SYSCALL 75
#define TRUNCATE_SYSCALL 76
#define FTRUNCATE_SYSCALL 77
#define GETDENTS_SYSCALL 78
#define GETCWD_SYSCALL 79
#define CHDIR_SYSCALL 80
#define FCHDIR_SYSCALL 81
#define RENAME_SYSCALL 82
#define MKDIR_SYSCALL 83
#define RMDIR_SYSCALL 84

#define LINK_SYSCALL 86
#define UNLINK_SYSCALL 87

#define READLINK_SYSCALL 89
#define CHMOD_SYSCALL 90
#define FCHMOD_SYSCALL 91

#define GETUID_SYSCALL 102
#define GETGID_SYSCALL 104
#define GETEUID_SYSCALL 107
#define GETEGID_SYSCALL 108
#define GETPPID_SYSCALL 110
#define STATFS_SYSCALL 137
#define FSTATFS_SYSCALL 138
#define GETHOSTNAME_SYSCALL 170
#define FUTEX_SYSCALL 202
#define EPOLL_CREATE_SYSCALL 213
#define CLOCK_GETTIME_SYSCALL 228
#define EPOLL_WAIT_SYSCALL 232
#define EPOLL_CTL_SYSCALL 233
#define UNLINKAT_SYSCALL 263
#define READLINKAT_SYSCALL 267
#define SYNC_FILE_RANGE 277
#define DUP3_SYSCALL 292
#define PIPE2_SYSCALL 293

#define SBRK_SYSCALL 1004

#endif /* _LIND_SYSCALL_NUM_H */
 
