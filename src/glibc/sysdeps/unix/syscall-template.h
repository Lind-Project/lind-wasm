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
 * MAKE_TRANDITION:
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
#define MAKE_TRANDITION(syscall_num, syscall_name, arg1, arg2, arg3, arg4, arg5, arg6, raw_flag) \
    ({ \
        uint64_t __self = __lind_cageid; \
        MAKE_THREEI( \
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

/*
 * MAKE_THREEI:
 *
 * Unified macro used to invoke threei style syscalls.  This is the
 * core entry point for all syscall transitions into the lind runtime,
 * including inter-cage calls and grates.  Unlike MAKE_TRANDITION, this macro
 * explicitly specifies both `self_cageid` and `target_cageid`, allowing
 * fine-grained routing of syscalls across cage boundaries.
 *
 * Each logical argument is passed in a (value, cageid) pair, enabling
 * three-i's interposition layer to perform selective rewriting, mediation,
 * or redirection.  The final argument, `raw_flag`, determines whether
 * lind_syscall should apply standard POSIX errno translation or return
 * the raw trampoline result directly.
 *
 * MAKE_THREEI is designed to be the **canonical** macro for all new
 * inter-cage or grate-level syscall invocations.  Grates and higher-level
 * components should call MAKE_THREEI directly rather than relying on
 * MAKE_TRANDITION.
 */
#define MAKE_THREEI( \
    syscall_num, \
    syscall_name, \
    self_cageid, target_cageid, \
    arg1, arg1_cageid, \
    arg2, arg2_cageid, \
    arg3, arg3_cageid, \
    arg4, arg4_cageid, \
    arg5, arg5_cageid, \
    arg6, arg6_cageid, \
    raw_flag \
) \
    lind_syscall( \
        (uint64_t)(syscall_num), \
        (uint64_t)(syscall_name), \
        (uint64_t)(self_cageid), \
        (uint64_t)(target_cageid), \
        (uint64_t)(arg1), (uint64_t)(arg1_cageid), \
        (uint64_t)(arg2), (uint64_t)(arg2_cageid), \
        (uint64_t)(arg3), (uint64_t)(arg3_cageid), \
        (uint64_t)(arg4), (uint64_t)(arg4_cageid), \
        (uint64_t)(arg5), (uint64_t)(arg5_cageid), \
        (uint64_t)(arg6), (uint64_t)(arg6_cageid), \
        (uint64_t)(raw_flag) \
    )

#define REGISTER_HANDLER_SYSCALL(targetcage, targetcallnum, handlefunc_index_in_this_grate, this_grate_id, optional_arg) \
    lind_register_syscall((uint64_t) targetcage, \
        (uint64_t) targetcallnum, \
        (uint64_t) handlefunc_flag, \
        (uint64_t) this_grate_id, \
        (uint64_t) optional_arg)

#define CP_DATA_SYSCALL(thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, copytype) \
    lind_cp_data((uint64_t) thiscage, \
        (uint64_t) targetcage, \
        (uint64_t) srcaddr, \
        (uint64_t) srccage, \
        (uint64_t) destaddr, \
        (uint64_t) destcage, \
        (uint64_t) len, \
        (uint64_t) copytype)
