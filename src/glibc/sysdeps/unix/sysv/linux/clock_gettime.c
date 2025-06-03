/* clock_gettime -- Get current time from a POSIX clockid_t.  Linux version.
   Copyright (C) 2003-2024 Free Software Foundation, Inc.
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

#include <sysdep.h>
#include <kernel-features.h>
#include <errno.h>
#include <time.h>
#include "kernel-posix-cpu-timers.h"
#include <sysdep-vdso.h>
#include <shlib-compat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

/* Get current value of CLOCK and store it in TP.  */

// Now after clang compile it will call gettime64 first(but in real case we have only 32 bit)
// In this case, we need “#if __TIMESIZE != 64” and this is always true. And always call __clock_gettime at last.

int
__clock_gettime64 (clockid_t clock_id, struct __timespec64 *tp)
{
  // return MAKE_SYSCALL(191, "syscall|clock_gettime", (uint64_t) clock_id, (uint64_t) tp, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  return MAKE_SYSCALL(CLOCK_GETTIME_SYSCALL, "syscall|clock_gettime", (uint64_t) clock_id, (uint64_t) tp, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

#if __TIMESIZE != 64
libc_hidden_def (__clock_gettime64)

int
__clock_gettime (clockid_t clock_id, struct timespec *tp)
{
  // return MAKE_SYSCALL(191, "syscall|clock_gettime", (uint64_t) clock_id, (uint64_t) tp, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  return MAKE_SYSCALL(CLOCK_GETTIME_SYSCALL, "syscall|clock_gettime", (uint64_t) clock_id, (uint64_t) tp, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

#endif
libc_hidden_def (__clock_gettime);

versioned_symbol (libc, __clock_gettime, clock_gettime, GLIBC_2_17);
/* clock_gettime moved to libc in version 2.17;
   old binaries may expect the symbol version it had in librt.  */
#if SHLIB_COMPAT (libc, GLIBC_2_2, GLIBC_2_17)
strong_alias (__clock_gettime, __clock_gettime_2);
compat_symbol (libc, __clock_gettime_2, clock_gettime, GLIBC_2_2);
#endif
