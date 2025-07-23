#include <sys/syscall.h>
#include <stdint.h> // For uint64_t
#include <unistd.h>
#include <lind_syscall.h>

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

#define MAKE_SYSCALL(syscallnum, callname, arg1, arg2, arg3, arg4, arg5, arg6) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), (unsigned long long)(arg5), (unsigned long long)(arg6))

#define REGISTER_HANDLER_SYSCALL(targetcage, targetcallnum, handlefunc_index_in_this_grate, this_grate_id) \
    lind_register_syscall((uint64_t) targetcage, \
        (uint64_t) targetcallnum, \
        (uint64_t) handlefunc_index_in_this_grate, \
        (uint64_t) this_grate_id)

#define CP_DATA_SYSCALL(thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, copytype) \
    lind_cp_data((uint64_t) thiscage, \
        (uint64_t) targetcage, \
        (uint64_t) srcaddr, \
        (uint64_t) srccage, \
        (uint64_t) destaddr, \
        (uint64_t) destcage, \
        (uint64_t) len, \
        (uint64_t) copytype)
