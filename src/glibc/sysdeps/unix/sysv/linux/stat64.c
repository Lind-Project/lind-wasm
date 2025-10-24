/* Get file status.  Linux version.
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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

#define __stat __redirect___stat
#define stat   __redirect_stat
#include <sys/stat.h>
#include <fcntl.h>
#include <kernel_stat.h>
#include <stat_t64_cp.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__stat64_time64 (const char *file, struct __stat64_t64 *buf)
{
  // BUG: we do not have fstatat syscall in rawposix
  // so let's just use xstat - Qianxi Chen
  return MAKE_SYSCALL(XSTAT_SYSCALL, "syscall|xstat", (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(file), (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(buf), NOTUSED, NOTUSED, NOTUSED, NOTUSED);

}
#if __TIMESIZE != 64
hidden_def (__stat64_time64)

int
__stat64 (const char *file, struct stat64 *buf)
{
  struct __stat64_t64 st_t64;
  return __stat64_time64 (file, &st_t64)
	 ?: __cp_stat64_t64_stat64 (&st_t64, buf);
}
#endif

#undef __stat
#undef stat

hidden_def (__stat64)
weak_alias (__stat64, stat64)

#if XSTAT_IS_XSTAT64
strong_alias (__stat64, __stat)
weak_alias (__stat64, stat)
#endif
