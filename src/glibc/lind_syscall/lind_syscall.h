#ifndef _LIND_SYSCALL_H
#define _LIND_SYSCALL_H

/*
 * lind_syscall.h
 *
 * Public threei / grate interface header.
 *
 * This header exposes the minimal C API that user code (grates and other
 * interposition components) can rely on when running inside the lind-wasm
 * sysroot.  It is intended to be installed into the sysroot include path and
 * used by grate authors to:
 *
 *   - Invoke threei style syscalls via make_threei_call().
 *   - Register or deregister grate-level syscall handlers via register_handler().
 *   - Copy data between cages in a controlled way via copy_data_between_cages().
 */

#include <stdint.h> // For uint64_t definition

/*
 * Flag to mark an argcageid as pointing into the grate's own linear memory
 * rather than the calling cage's memory. OR this with the cageid when passing
 * a pointer that lives in the grate's address space.
 *
 * Example: arg2cageid = my_cageid | GRATE_MEMORY_FLAG
 */
#define GRATE_MEMORY_FLAG ((uint64_t)1 << 63)

/*
 * Raw error codes returned by the 3i calls below (they do not set errno).
 * Compare against the return value cast to int, e.g. ret == (int)ELINDEPERM.
 *
 *   ELINDAPIABORTED: invalid arguments or state
 *   ELINDESRCH:      a cage the call names is exiting
 *   ELINDEPERM:      the calling cage may not reference a cage the call names
 *                    (only itself, its descendants and cages granted to it),
 *                    or may not grant/revoke the requested access
 *
 * An ordinary syscall denied for the same reason fails with errno EPERM.
 */
#define ELINDAPIABORTED 0xE0010001U
#define ELINDESRCH      0xE0010002U
#define ELINDEPERM      0xE0010003U

int make_threei_call (unsigned int callnumber, 
    uint64_t callname, 
    uint64_t self_cageid, uint64_t target_cageid,
    uint64_t arg1, uint64_t arg1cageid,
    uint64_t arg2, uint64_t arg2cageid,
    uint64_t arg3, uint64_t arg3cageid,
    uint64_t arg4, uint64_t arg4cageid,
    uint64_t arg5, uint64_t arg5cageid,
    uint64_t arg6, uint64_t arg6cageid,
    int translate_errno);

int register_handler(uint64_t targetcage, 
    uint64_t targetcallnum, 
    uint64_t this_grate_id,
    uint64_t in_grate_fn_ptr_u64);
    
int copy_data_between_cages(uint64_t thiscage, uint64_t targetcage, 
    uint64_t srcaddr, uint64_t srccage, 
    uint64_t destaddr, uint64_t destcage, 
    uint64_t len, uint64_t copytype);

int copy_handler_table_to_cage(uint64_t srccage, uint64_t targetcage);

int grant_cage_access(uint64_t subject, uint64_t grantee);

int revoke_cage_access(uint64_t subject, uint64_t grantee);
#endif // _LIND_SYSCALL_H
