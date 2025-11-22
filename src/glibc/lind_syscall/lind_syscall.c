#include <errno.h>
#include <stdint.h> // For uint64_t definition

// Entry point for wasmtime, lind_syscall is an imported function from wasmtime
int __lind_make_syscall_trampoline(unsigned int callnumber, 
    uint64_t callname, 
    uint64_t self_cageid, uint64_t target_cageid,
    uint64_t arg1, uint64_t arg1cageid,
    uint64_t arg2, uint64_t arg2cageid,
    uint64_t arg3, uint64_t arg3cageid,
    uint64_t arg4, uint64_t arg4cageid,
    uint64_t arg5, uint64_t arg5cageid,
    uint64_t arg6, uint64_t arg6cageid
) __attribute__((
    __import_module__("lind"),
    __import_name__("make-syscall")
));

/*
 * make_threei:
 *
 * Unified function used to invoke threei style syscalls.  This is the
 * core entry point for all syscall transitions into the lind runtime,
 * including inter-cage calls and grates.  Unlike MAKE_TRADITION, this function
 * explicitly specifies both `self_cageid` and `target_cageid`, allowing
 * fine-grained routing of syscalls across cage boundaries.
 *
 * Each logical argument is passed in a (value, cageid) pair, enabling
 * three-i's interposition layer to perform selective rewriting, mediation,
 * or redirection.  The final argument, `raw_flag`, determines whether
 * lind_syscall should apply standard POSIX errno translation or return
 * the raw trampoline result directly.
 *
 * make_threei is designed to be the **canonical** macro for all new
 * inter-cage or grate-level syscall invocations.  Grates and higher-level
 * components should call make_threei directly rather than relying on
 * MAKE_TRADITION.
 *
 * This function acts as the middle layer between:
 *   (1) the MAKE_TRADITION macros in glibc, and
 *   (2) the actual Wasmtime entry function (__lind_make_syscall_trampoline).
 *
 * It forwards all syscall parameters—including the inter-cage metadata
 * (self_cageid, target_cageid, argX_cageid pairs) to the underlying
 * trampoline, but also optionally performs post-processing on the return
 * value depending on `raw_flag`.
 *
 * The `raw_flag` controls whether this wrapper should apply the standard
 * errno handling:
 *
 *   raw_flag == 0:
 *       The return value is treated as a complete syscall result.
 *       Negative values in the range [-255, -1] are interpreted as
 *       `-errno`, errno is set accordingly, and the wrapper returns -1.
 *       All other values are returned directly.
 *
 *   raw_flag == 1:
 *       The wrapper does *not* apply any errno translation.
 *       The raw return value from the trampoline is returned as-is.
 *
 * This distinction is required because some syscalls—especially futex-related
 * operations (e.g., lll_futex_wake, lll_futex_requeue, etc.) expect the
 * trampoline to return raw -errno value and must not receive additional errno 
 * post-processing at this layer. Other syscalls, however, rely on the standard 
 * POSIX errno translation implemented here.
 */
int make_threei (unsigned int callnumber, 
    uint64_t callname, 
    uint64_t self_cageid, uint64_t target_cageid,
    uint64_t arg1, uint64_t arg1cageid,
    uint64_t arg2, uint64_t arg2cageid,
    uint64_t arg3, uint64_t arg3cageid,
    uint64_t arg4, uint64_t arg4cageid,
    uint64_t arg5, uint64_t arg5cageid,
    uint64_t arg6, uint64_t arg6cageid,
    int raw)
{
    int ret = __lind_make_syscall_trampoline(callnumber, 
        callname, 
        self_cageid, target_cageid,
        arg1, arg1cageid,
        arg2, arg2cageid,
        arg3, arg3cageid,
        arg4, arg4cageid,
        arg5, arg5cageid,
        arg6, arg6cageid);
    // if raw is set, we do not do any further process to errno handling and directly return the result
    if(raw != 0) return ret;
    // handle the errno
    // in rawposix, we use -errno as the return value to indicate the error
    // but this may cause some issues for mmap syscall, because mmap syscall
    // is returning an 32-bit address, which may overflow the int type (i32)
    // luckily we can handle this easily because the return value of mmap is always
    // multiple of pages (typically 4096) even when overflow, therefore we can distinguish
    // the errno and mmap result by simply checking if the return value is
    // within the valid errno range
    if(ret < 0 && ret > -256)
    {
        errno = -ret;
        return -1;
    }
    else
    {
        errno = 0;
    }
    return ret;
}

// ---------------------------------------------------------------------------------------------------------------------

// Entry point for wasmtime, lind_syscall is an imported function from wasmtime
int __imported_lind_3i_trampoline_register_syscall(uint64_t targetcage, 
    uint64_t targetcallnum, 
    uint64_t handlefunc_flag, 
    uint64_t this_grate_id,
    uint64_t optional_arg) __attribute__((
    __import_module__("lind"),
    __import_name__("register-syscall")
));

// 3i function call to register or deregister a syscall handler in a target cage
// targetcage: the cage id where the syscall will be registered
// targetcallnum: the syscall number to be registered in the target cage
// this_grate_id: the grate id of the syscall jump ends
// register_flag: deregister(0) or register(non-0)
int register_handler (int64_t targetcage, 
    uint64_t targetcallnum, 
    uint64_t handlefunc_flag, 
    uint64_t this_grate_id,
    uint64_t optional_arg)
{
    int ret = __imported_lind_3i_trampoline_register_syscall(targetcage, targetcallnum, handlefunc_flag, this_grate_id, optional_arg);
    
    return ret;
}

// ---------------------------------------------------------------------------------------------------------------------
// Entry point for wasmtime, lind_cp_data is an imported function from wasmtime
int __imported_lind_3i_trampoline_cp_data(uint64_t thiscage, uint64_t targetcage, uint64_t srcaddr, uint64_t srccage, uint64_t destaddr, uint64_t destcage, uint64_t len, uint64_t copytype) __attribute__((
    __import_module__("lind"),
    __import_name__("cp-data-syscall")
));

// 3i function call to copy data between cages
// thiscage: the cage id of the caller cage
// targetcage: the cage id of the target cage
// srcaddr: the source address to copy from
// srccage: the cage id of the source address
// destaddr: the destination address to copy to
// destcage: the cage id of the destination address
// len: the length of data to copy
// copytype: the type of copy, 0 for normal copy, 1 for string copy
int copy_data_between_cages(uint64_t thiscage, uint64_t targetcage, uint64_t srcaddr, uint64_t srccage, uint64_t destaddr, uint64_t destcage, uint64_t len, uint64_t copytype)
{
    int ret = __imported_lind_3i_trampoline_cp_data(thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, copytype);
    
    return ret;
}
