/* Get file status.
   Copyright (C) 1996-2024 Free Software Foundation, Inc.
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

#define __lstat __redirect___lstat
#define lstat __redirect_lstat
#include <sys/stat.h>
#include <fcntl.h>
#include <kernel_stat.h>
#include <stat_t64_cp.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__lstat64_time64 (const char *file, struct __stat64_t64 *buf)
{
  // BUG: we do not have fstatat syscall in rawposix
  // so let's just use xstat - Qianxi Chen
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  return MAKE_SYSCALL (XSTAT_SYSCALL, "syscall|xstat",
		       host_file, host_buf,
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
#if __TIMESIZE != 64
hidden_def (__lstat64_time64)

int
__lstat64 (const char *file, struct stat64 *buf)
{
  return MAKE_SYSCALL2(XSTAT_SYSCALL, "syscall|xstat", (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(file), (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(buf)); 
}
#endif
hidden_def (__lstat64)
weak_alias (__lstat64, lstat64)

#undef __lstat
#undef lstat

#if XSTAT_IS_XSTAT64
strong_alias (__lstat64, __lstat)
weak_alias (__lstat64, lstat)
#endif
