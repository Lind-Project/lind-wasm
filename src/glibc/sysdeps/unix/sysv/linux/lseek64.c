/* Linux lseek implementation, 64 bits off_t.
   Copyright (C) 2016-2024 Free Software Foundation, Inc.
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

#include <unistd.h>
#include <stdint.h>
#include <sys/types.h>
#include <sysdep.h>
#include <errno.h>
#include <shlib-compat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

off64_t
__lseek64 (int fd, off64_t offset, int whence)
{
   return MAKE_SYSCALL(LSEEK_SYSCALL, "syscall|lseek", (uint64_t) fd, (uint64_t) offset, (uint64_t) whence, NOTUSED, NOTUSED, NOTUSED);
}

#ifdef  __OFF_T_MATCHES_OFF64_T
weak_alias (__lseek64, lseek)
weak_alias (__lseek64, __lseek)
strong_alias (__lseek64, __libc_lseek)
libc_hidden_def (__lseek)
#endif

strong_alias (__lseek64, __libc_lseek64)
weak_alias (__lseek64, lseek64)

#if SHLIB_COMPAT (libc, GLIBC_2_0, GLIBC_2_28)
compat_symbol (libc, __lseek64, llseek, GLIBC_2_0);
#endif

#if !IS_IN(rtld) && OTHER_SHLIB_COMPAT (libpthread, GLIBC_2_1, GLIBC_2_2)
compat_symbol (libc, __lseek64, lseek64, GLIBC_2_2);
#endif
