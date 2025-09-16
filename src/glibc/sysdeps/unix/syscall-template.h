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

#define REGISTER_HANDLER_SYSCALL(targetcage, targetcallnum, handlefunc_index_in_this_grate, this_grate_id) \
    lind_register_syscall((uint64_t) targetcage, \
        (uint64_t) targetcallnum, \
        (uint64_t) this_grate_id, \
        (uint64_t) register_flag)

#define CP_DATA_SYSCALL(thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, copytype) \
    lind_cp_data((uint64_t) thiscage, \
        (uint64_t) targetcage, \
        (uint64_t) srcaddr, \
        (uint64_t) srccage, \
        (uint64_t) destaddr, \
        (uint64_t) destcage, \
        (uint64_t) len, \
        (uint64_t) copytype)
