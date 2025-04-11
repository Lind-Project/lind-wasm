/*
 * Lind syscall number definitions
 *
 * This file contains syscall number constants used by Lind WASM to map
 * between glibc function calls and the corresponding system calls.
 *
 * These syscall numbers also align with the Linux syscall table.
 * Reference: https://github.com/torvalds/linux/blob/master/arch/x86/entry/syscalls/syscall_64.tbl
 */

 #ifndef _LIND_SYSCALL_NUM_H
 #define _LIND_SYSCALL_NUM_H
 
 #define ACCESS_SYSCALL 21
 #define UNLINKAT_SYSCALL 263
 #define UNLINK_SYSCALL 87
 #define LINK_SYSCALL 86
 #define RENAME_SYSCALL 82
 
 #define XSTAT_SYSCALL 4
 #define OPEN_SYSCALL 2
 #define CLOSE_SYSCALL 3
 #define READ_SYSCALL 0
 #define WRITE_SYSCALL 1
 #define LSEEK_SYSCALL 8
 #define IOCTL_SYSCALL 16
 #define TRUNCATE_SYSCALL 76
 #define FXSTAT_SYSCALL 5
 #define FTRUNCATE_SYSCALL 77
 #define FSTATFS_SYSCALL 138
 #define MMAP_SYSCALL 9
 #define MUNMAP_SYSCALL 11
 #define MPROTECT_SYSCALL 10
 #define GETDENTS_SYSCALL 78
 #define DUP_SYSCALL 32
 #define DUP2_SYSCALL 33
 #define STATFS_SYSCALL 137
 #define FCNTL_SYSCALL 72
 
 #define GETPPID_SYSCALL 110
 #define EXIT_SYSCALL 60
 #define GETPID_SYSCALL 39
 
 #define BIND_SYSCALL 49
 #define SEND_SYSCALL 46
 #define SENDTO_SYSCALL 44
 #define RECV_SYSCALL 47
 #define RECVFROM_SYSCALL 45
 #define CONNECT_SYSCALL 42
 #define LISTEN_SYSCALL 50
 #define ACCEPT_SYSCALL 43
 
 #define GETSOCKOPT_SYSCALL 55
 #define SETSOCKOPT_SYSCALL 54
 #define SHUTDOWN_SYSCALL 48
 #define SELECT_SYSCALL 23
 #define GETCWD_SYSCALL 79
 #define POLL_SYSCALL 7
 #define SOCKETPAIR_SYSCALL 53
 #define GETUID_SYSCALL 102
 #define GETEUID_SYSCALL 107
 #define GETGID_SYSCALL 104
 #define GETEGID_SYSCALL 108
 #define FLOCK_SYSCALL 73
 #define EPOLL_CREATE_SYSCALL 213
 #define EPOLL_CTL_SYSCALL 233
 #define EPOLL_WAIT_SYSCALL 232
 
 #define SHMGET_SYSCALL 29
 #define SHMAT_SYSCALL 30
 #define SHMDT_SYSCALL 67
 #define SHMCTL_SYSCALL 31
 
 #define PIPE_SYSCALL 22
 #define PIPE2_SYSCALL 293
 #define FORK_SYSCALL 57
 #define EXEC_SYSCALL 59
 
 #define MUTEX_CREATE_SYSCALL 1000
 #define COND_CREATE_SYSCALL 1001
 #define COND_TIMEDWAIT_SYSCALL 1002
 
 #define SEM_TIMEDWAIT_SYSCALL 1003
 #define FUTEX_SYSCALL 202
 
 #define GETHOSTNAME_SYSCALL 170
 #define PREAD_SYSCALL 17
 #define PWRITE_SYSCALL 18
 #define CHDIR_SYSCALL 80
 #define MKDIR_SYSCALL 83
 #define RMDIR_SYSCALL 84
 #define CHMOD_SYSCALL 90
 #define FCHMOD_SYSCALL 91
 
 #define SOCKET_SYSCALL 41
 
 #define GETSOCKNAME_SYSCALL 51
 #define GETPEERNAME_SYSCALL 52
 
 #define SIGACTION_SYSCALL 13
 #define KILL_SYSCALL 62
 #define SIGPROCMASK_SYSCALL 14
 #define SETITIMER_SYSCALL 38
 
 #define FCHDIR_SYSCALL 81
 #define FSYNC_SYSCALL 74
 #define FDATASYNC_SYSCALL 75
 #define SYNC_FILE_RANGE 277
 
 #define READLINK_SYSCALL 89
 #define READLINKAT_SYSCALL 267
 
 #define WRITEV_SYSCALL 20
 
 #define CLONE_SYSCALL 56
 #define EXECVE_SYSCALL 59
 #define WAIT_SYSCALL 61
 #define WAITPID_SYSCALL 61
 #define BRK_SYSCALL 12
 #define SBRK_SYSCALL 1004
 
 #define NANOSLEEP_TIME64_SYSCALL 35
 #define CLOCK_GETTIME_SYSCALL 228
 
 #endif /* _LIND_SYSCALL_NUM_H */
 
