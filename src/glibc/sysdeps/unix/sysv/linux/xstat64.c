/* xstat64 using Linux stat64 system call.
   Copyright (C) 1991-2024 Free Software Foundation, Inc.
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
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#define __xstat __redirect___xstat
#include <sys/stat.h>
#undef __xstat
#include <fcntl.h>
#include <kernel_stat.h>
#include <sysdep.h>
#include <xstatconv.h>
#include <statx_cp.h>
#include <shlib-compat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

#if LIB_COMPAT(libc, GLIBC_2_0, GLIBC_2_33)

/* Get information about the file NAME in BUF.  */

int
___xstat64 (int vers, const char *name, struct stat64 *buf)
{
   // return MAKE_SYSCALL(9, "syscall|xstat", (uint64_t) vers, (uint64_t) name, (uint64_t) buf, NOTUSED, NOTUSED, NOTUSED);
   return MAKE_SYSCALL(XSTAT_SYSCALL, "syscall|xstat", (uint64_t) vers, (uint64_t) name, (uint64_t) buf, NOTUSED, NOTUSED, NOTUSED);
}

#if XSTAT_IS_XSTAT64
strong_alias (___xstat64, __xstat)
#endif

#if SHLIB_COMPAT(libc, GLIBC_2_1, GLIBC_2_2)
versioned_symbol (libc, ___xstat64, __xstat64, GLIBC_2_2);
strong_alias (___xstat64, __old__xstat64)
compat_symbol (libc, __old__xstat64, __xstat64, GLIBC_2_1);
#else
strong_alias (___xstat64, __xstat64)
#endif


#endif /* LIB_COMPAT  */
