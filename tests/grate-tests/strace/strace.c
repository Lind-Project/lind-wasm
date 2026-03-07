#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include "strace.h"

// table for storing syscall handlers
syscall_handler_t syscall_handler_table[MAX_SYSCALLS] = {0};

// defined syscall handlers
//
// args:
//      1st:            syscall name
//      2nd:            syscall number
//      3rd - 8th:      ARG type (ARG_INT || ARG_PTR || ARG_STR)
//
// NOTE: if unsure of ARG_TYPE follow:
// https://www.chromium.org/chromium-os/developer-library/reference/linux-constants/syscalls/
//
// defines handler for all syscalls supported by lind

DEFINE_HANDLER(read, 0, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(write, 1, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(open, 2, ARG_STR, ARG_INT, ARG_INT)
DEFINE_HANDLER(close, 3, ARG_INT)
DEFINE_HANDLER(stat, 4, ARG_STR, ARG_PTR)
DEFINE_HANDLER(fstat, 5, ARG_INT, ARG_PTR)
DEFINE_HANDLER(poll, 7, ARG_PTR, ARG_INT, ARG_INT)
DEFINE_HANDLER(lseek, 8, ARG_INT, ARG_INT, ARG_INT)
DEFINE_HANDLER(mmap, 9, ARG_PTR, ARG_INT, ARG_INT, ARG_INT, ARG_INT, ARG_INT)
DEFINE_HANDLER(mprotect, 10, ARG_PTR, ARG_INT, ARG_INT)
DEFINE_HANDLER(munmap, 11, ARG_PTR, ARG_INT)
DEFINE_HANDLER(brk, 12, ARG_PTR)
DEFINE_HANDLER(sigaction, 13, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(sigprocmask, 14, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(ioctl, 16, ARG_INT, ARG_INT, ARG_PTR)
DEFINE_HANDLER(pread, 17, ARG_INT, ARG_PTR, ARG_INT, ARG_INT)
DEFINE_HANDLER(pwrite, 18, ARG_INT, ARG_PTR, ARG_INT, ARG_INT)
DEFINE_HANDLER(writev, 20, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(access, 21, ARG_STR, ARG_INT)
DEFINE_HANDLER(pipe, 22, ARG_PTR)
DEFINE_HANDLER(select, 23, ARG_INT, ARG_PTR, ARG_PTR, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(sched_yield, 24)
DEFINE_HANDLER(shmget, 29, ARG_INT, ARG_INT, ARG_INT)
DEFINE_HANDLER(shmat, 30, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(shmctl, 31, ARG_INT, ARG_INT, ARG_PTR)
DEFINE_HANDLER(dup, 32, ARG_INT)
DEFINE_HANDLER(dup2, 33, ARG_INT, ARG_INT)
DEFINE_HANDLER(nanosleep, 35, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(setitimer, 38, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(getpid, 39)
DEFINE_HANDLER(socket, 41, ARG_INT, ARG_INT, ARG_INT)
DEFINE_HANDLER(connect, 42, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(accept, 43, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(sendto, 44, ARG_INT, ARG_PTR, ARG_INT, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(recvfrom, 45, ARG_INT, ARG_PTR, ARG_INT, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(shutdown, 48, ARG_INT, ARG_INT)
DEFINE_HANDLER(bind, 49, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(listen, 50, ARG_INT, ARG_INT)
DEFINE_HANDLER(getsockname, 51, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(getpeername, 52, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(socketpair, 53, ARG_INT, ARG_INT, ARG_INT, ARG_PTR)
DEFINE_HANDLER(setsockopt, 54, ARG_INT, ARG_INT, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(getsockopt, 55, ARG_INT, ARG_INT, ARG_INT, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(clone, 56, ARG_INT, ARG_PTR, ARG_PTR, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(fork, 57)
DEFINE_HANDLER(exec, 59, ARG_STR, ARG_PTR, ARG_PTR)
DEFINE_HANDLER(exit, 60, ARG_INT)
DEFINE_HANDLER(waitpid, 61, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(kill, 62, ARG_INT, ARG_INT)
DEFINE_HANDLER(shmdt, 67, ARG_PTR)
DEFINE_HANDLER(fcntl, 72, ARG_INT, ARG_INT, ARG_PTR)
DEFINE_HANDLER(flock, 73, ARG_INT, ARG_INT)
DEFINE_HANDLER(fsync, 74, ARG_INT)
DEFINE_HANDLER(fdatasync, 75, ARG_INT)
DEFINE_HANDLER(truncate, 76, ARG_STR, ARG_INT)
DEFINE_HANDLER(ftruncate, 77, ARG_INT, ARG_INT)
DEFINE_HANDLER(getdents, 78, ARG_INT, ARG_PTR, ARG_INT)
DEFINE_HANDLER(getcwd, 79, ARG_PTR, ARG_INT)
DEFINE_HANDLER(chdir, 80, ARG_STR)
DEFINE_HANDLER(fchdir, 81, ARG_INT)
DEFINE_HANDLER(rename, 82, ARG_STR, ARG_STR)
DEFINE_HANDLER(mkdir, 83, ARG_STR, ARG_INT)
DEFINE_HANDLER(rmdir, 84, ARG_STR)
DEFINE_HANDLER(link, 86, ARG_STR, ARG_STR)
DEFINE_HANDLER(unlink, 87, ARG_STR)
DEFINE_HANDLER(readlink, 89, ARG_STR, ARG_PTR, ARG_INT)
DEFINE_HANDLER(chmod, 90, ARG_STR, ARG_INT)
DEFINE_HANDLER(fchmod, 91, ARG_INT, ARG_INT)
DEFINE_HANDLER(getuid, 102)
DEFINE_HANDLER(getgid, 104)
DEFINE_HANDLER(geteuid, 107)
DEFINE_HANDLER(getegid, 108)
DEFINE_HANDLER(getppid, 110)
DEFINE_HANDLER(statfs, 137, ARG_STR, ARG_PTR)
DEFINE_HANDLER(fstatfs, 138, ARG_INT, ARG_PTR)
DEFINE_HANDLER(gethostname, 170, ARG_PTR, ARG_INT)
DEFINE_HANDLER(futex, 202, ARG_PTR, ARG_INT, ARG_INT, ARG_PTR, ARG_PTR, ARG_INT)
DEFINE_HANDLER(epoll_create, 213, ARG_INT)
DEFINE_HANDLER(clock_gettime, 228, ARG_INT, ARG_PTR)
DEFINE_HANDLER(epoll_wait, 232, ARG_INT, ARG_PTR, ARG_INT, ARG_INT)
DEFINE_HANDLER(epoll_ctl, 233, ARG_INT, ARG_INT, ARG_INT, ARG_PTR)
DEFINE_HANDLER(unlinkat, 263, ARG_INT, ARG_STR, ARG_INT)
DEFINE_HANDLER(readlinkat, 267, ARG_INT, ARG_STR, ARG_PTR, ARG_INT)
DEFINE_HANDLER(sync_file_range, 277, ARG_INT, ARG_INT, ARG_INT, ARG_INT)
DEFINE_HANDLER(epoll_create1, 291, ARG_INT)
DEFINE_HANDLER(dup3, 292, ARG_INT, ARG_INT, ARG_INT)
DEFINE_HANDLER(pipe2, 293, ARG_PTR, ARG_INT)
DEFINE_HANDLER(getrandom, 318, ARG_PTR, ARG_INT, ARG_INT)

// dispatcher function
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage, 
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, 
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage, 
                    uint64_t arg6, uint64_t arg6cage) {

    if (fn_ptr_uint == 0) {
        return -1;
    }

    syscall_handler_t fn = (syscall_handler_t)(uintptr_t)fn_ptr_uint;

    return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, 
              arg4, arg4cage, arg5, arg5cage, arg6, arg6cage);
}
