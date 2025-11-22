#include <sys/syscall.h>
#include <stdint.h> // For uint64_t
#include <unistd.h>
#include <lind_syscall.h>
#include <addr_translation.h>

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

#define WRAPPED_SYSCALL 0
#define RAW_SYSCALL 1

/*
 * MAKE_TRADITION:
 *
 * Legacy macro used inside the glibc compatibility layer for traditional
 * POSIX-style syscalls.  This macro wraps a syscall using the calling
 * convention expected by legacy threei code, and automatically assigns
 * both the `self_cageid` and `target_cageid` to the current cage
 * (`__lind_cageid`).
 *
 * Arguments (arg1..arg6) are paired with the caller's cage ID and forwarded
 * directly into MAKE_THREEI, which performs the actual three-i style call.
 *
 * In the long term, this macro exists solely for backward compatibility
 * with glibc and legacy POSIX implementations.  New subsystems (e.g., grates)
 * should use MAKE_THREEI directly.
 */
#define MAKE_TRADITION(syscall_num, syscall_name, arg1, arg2, arg3, arg4, arg5, arg6, raw_flag) \
    ({ \
        uint64_t __self = __lind_cageid; \
        make_threei( \
            (syscall_num), \
            (syscall_name), \
            __self, /* self_cageid */ \
            __self, /* target_cageid: same as self for traditional syscalls */ \
            (uint64_t)(arg1), __self, \
            (uint64_t)(arg2), __self, \
            (uint64_t)(arg3), __self, \
            (uint64_t)(arg4), __self, \
            (uint64_t)(arg5), __self, \
            (uint64_t)(arg6), __self, \
            (raw_flag) \
        ); \
    })
