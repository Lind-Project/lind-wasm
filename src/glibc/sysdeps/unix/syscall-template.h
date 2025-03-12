#include <sys/syscall.h>
#include <stdint.h> // For uint64_t
#include <unistd.h>
#include <lind_syscall.h>

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

#define WARPPED_SYSCALL 0
#define RAW_SYSCALL 1

#define MAKE_SYSCALL6(syscallnum, callname, arg1, arg2, arg3, arg4, arg5, arg6) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), (unsigned long long)(arg5), (unsigned long long)(arg6), WARPPED_SYSCALL)

#define MAKE_SYSCALL5(syscallnum, callname, arg1, arg2, arg3, arg4, arg5) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
            (unsigned long long)(arg4), (unsigned long long)(arg5), (unsigned long long)(NOTUSED), WARPPED_SYSCALL)

#define MAKE_SYSCALL4(syscallnum, callname, arg1, arg2, arg3, arg4) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), WARPPED_SYSCALL)

#define MAKE_SYSCALL3(syscallnum, callname, arg1, arg2, arg3) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), WARPPED_SYSCALL)

#define MAKE_SYSCALL2(syscallnum, callname, arg1, arg2) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(NOTUSED), \
             (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), WARPPED_SYSCALL)

#define MAKE_SYSCALL1(syscallnum, callname, arg1) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), \
             (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), WARPPED_SYSCALL)

#define MAKE_SYSCALL0(syscallnum, callname) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), \
             (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), WARPPED_SYSCALL)

#define MAKE_SYSCALL MAKE_SYSCALL6

#define MAKE_RAW_SYSCALL6(syscallnum, callname, arg1, arg2, arg3, arg4, arg5, arg6) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), (unsigned long long)(arg5), (unsigned long long)(arg6), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL5(syscallnum, callname, arg1, arg2, arg3, arg4, arg5) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
            (unsigned long long)(arg4), (unsigned long long)(arg5), (unsigned long long)(NOTUSED), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL4(syscallnum, callname, arg1, arg2, arg3, arg4) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL3(syscallnum, callname, arg1, arg2, arg3) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL2(syscallnum, callname, arg1, arg2) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(NOTUSED), \
             (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL1(syscallnum, callname, arg1) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), \
             (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL0(syscallnum, callname) \
lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), \
             (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), (unsigned long long)(NOTUSED), RAW_SYSCALL)

#define MAKE_RAW_SYSCALL MAKE_RAW_SYSCALL6
