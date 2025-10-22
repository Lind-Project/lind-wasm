#include <errno.h>
#include <stdint.h> // For uint64_t definition
/* Indirect system call.  Linux generic implementation.
   Copyright (C) 1997-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   The GNU C Library is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 2.1 of the License, or (at your option) any later version.

   The GNU C Library is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

// Entry point for wasmtime, lind_syscall is an imported function from wasmtime
int __imported_wasi_snapshot_preview1_lind_syscall(unsigned int callnumber, unsigned long long callname, unsigned long long arg1, unsigned long long arg2, unsigned long long arg3, unsigned long long arg4, unsigned long long arg5, unsigned long long arg6) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-syscall")
));


// Part of Macro MAKE_SYSCALL, take in the number of the syscall and the name of the syscall and 6 argument.
// callnumber: is the syscall number used in rawposix/rustposix
// callname: a legacy argument, will be changed after 3i has integrated
// arg1-arg6: actual argument of the syscall, note that all the pointers passed here is 32-bit virtual wasm address
//            and should be handled appropriately. This might be changed later and the address translation might be
//            handled here instead
int lind_syscall (unsigned int callnumber, unsigned long long callname, unsigned long long arg1, unsigned long long arg2, unsigned long long arg3, unsigned long long arg4, unsigned long long arg5, unsigned long long arg6, int raw)
{
    int ret = __imported_wasi_snapshot_preview1_lind_syscall(callnumber, callname, arg1, arg2, arg3, arg4, arg5, arg6);
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
    uint64_t fn_ptr_u64) __attribute__((
    __import_module__("lind"),
    __import_name__("register-syscall")
));


// Shim between the user-facing 3i API (e.g., register_handler) and the
// Wasmtime trampoline import (__imported_lind_3i_trampoline_register_syscall).
// The `lind_` prefix marks this as a Lind-Wasm–specific runtime shim rather 
// than a generic/app symbol.
//
// 3i function call to register or deregister a syscall handler in a target cage
// targetcage: the cage id where the syscall will be registered
// targetcallnum: the syscall number to be registered in the target cage
// this_grate_id: the grate id of the syscall jump ends
// register_flag: deregister(0) or register(non-0)
int lind_register_syscall (int64_t targetcage, 
    uint64_t targetcallnum, 
    uint64_t handlefunc_flag, 
    uint64_t this_grate_id,
    uint64_t fn_ptr_u64)
{
    int ret = __imported_lind_3i_trampoline_register_syscall(targetcage, targetcallnum, handlefunc_flag, this_grate_id, fn_ptr_u64);
    
    return ret;
}

// ---------------------------------------------------------------------------------------------------------------------
// Entry point for wasmtime, lind_cp_data is an imported function from wasmtime
int __imported_lind_3i_trampoline_cp_data(uint64_t thiscage, uint64_t targetcage, uint64_t srcaddr, uint64_t srccage, uint64_t destaddr, uint64_t destcage, uint64_t len, uint64_t copytype) __attribute__((
    __import_module__("lind"),
    __import_name__("cp-data-syscall")
));

// Shim between the user-facing 3i API (e.g., register_handler) and the
// Wasmtime trampoline import (__imported_lind_3i_trampoline_register_syscall).
// The `lind_` prefix marks this as a Lind-Wasm–specific runtime shim rather 
// than a generic/app symbol.
//
// 3i function call to copy data between cages
// thiscage: the cage id of the caller cage
// targetcage: the cage id of the target cage
// srcaddr: the source address to copy from
// srccage: the cage id of the source address
// destaddr: the destination address to copy to
// destcage: the cage id of the destination address
// len: the length of data to copy
// copytype: the type of copy, 0 for normal copy, 1 for string copy
int lind_cp_data(uint64_t thiscage, uint64_t targetcage, uint64_t srcaddr, uint64_t srccage, uint64_t destaddr, uint64_t destcage, uint64_t len, uint64_t copytype)
{
    int ret = __imported_lind_3i_trampoline_cp_data(thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, copytype);
    
    return ret;
}
