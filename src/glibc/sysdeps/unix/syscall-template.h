#include <sys/syscall.h>
#include <stdint.h> // For uint64_t
#include <unistd.h>
#include <lind_syscall.h>
#include <addr_translation.h>

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

// Define flags for errno translation
// See comments in [`lind_syscall/lind_syscall.c`] for details
#define TRANSLATE_ERRNO_ON 1
#define TRANSLATE_ERRNO_OFF 0

/*
 * MAKE_LEGACY_SYSCALL:
 *
 * Legacy macro used inside the glibc compatibility layer for traditional
 * POSIX-style syscalls.  This macro wraps a syscall using the calling
 * convention expected by legacy threei code, and automatically assigns
 * both the `self_cageid` and `target_cageid` to the current cage
 * (`__lind_cageid`).
 *
 * Arguments (arg1..arg6) are paired with the caller's cage ID and forwarded
 * directly into make_threei_call, which performs the actual three-i style call.
 *
 * In the long term, this macro exists solely for backward compatibility
 * with glibc and legacy POSIX implementations.  New subsystems (e.g., grates)
 * should use make_threei_call directly.
 */
 /* todo: replace hardcoded RAWPOSIX_CAGEID with a constant */
#define MAKE_LEGACY_SYSCALL(syscall_num, syscall_name, arg1, arg2, arg3, arg4, arg5, arg6, translate_errno) \
    ({ \
        uint64_t __self = __lind_cageid; \
        make_threei_call( \
            (syscall_num), \
            (syscall_name), \
            __self, /* self_cageid */ \
            777777, /* target_cageid: set to RAWPOSIX_CAGEID by default */ \
            (uint64_t)(arg1), __self, \
            (uint64_t)(arg2), __self, \
            (uint64_t)(arg3), __self, \
            (uint64_t)(arg4), __self, \
            (uint64_t)(arg5), __self, \
            (uint64_t)(arg6), __self, \
            (translate_errno) \
        ); \
    })
