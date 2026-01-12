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
    uint64_t handlefunc_flag,
    uint64_t this_grate_id,
    uint64_t optional_arg);
    
int copy_data_between_cages(uint64_t thiscage, uint64_t targetcage, 
    uint64_t srcaddr, uint64_t srccage, 
    uint64_t destaddr, uint64_t destcage, 
    uint64_t len, uint64_t copytype);

#endif // _LIND_SYSCALL_H
