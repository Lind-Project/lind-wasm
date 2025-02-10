#include <errno.h>
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
int lind_syscall (unsigned int callnumber, unsigned long long callname, unsigned long long arg1, unsigned long long arg2, unsigned long long arg3, unsigned long long arg4, unsigned long long arg5, unsigned long long arg6)
{
  int ret = __imported_wasi_snapshot_preview1_lind_syscall(callnumber, callname, arg1, arg2, arg3, arg4, arg5, arg6);
  // handle the errno
  if(ret < 0)
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
