#include <sys/syscall.h>
#include <stdint.h> // For uint64_t
#include <unistd.h>
#include <lind_syscall.h>

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

#define MAKE_SYSCALL(syscallnum, callname, arg1, arg2, arg3, arg4, arg5, arg6) \
    lind_syscall(syscallnum, (unsigned long long)(callname), (unsigned long long)(arg1), (unsigned long long)(arg2), (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), (unsigned long long)(arg5), (unsigned long long)(arg6))


