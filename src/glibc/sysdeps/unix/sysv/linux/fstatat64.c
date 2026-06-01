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

#define __fstatat __redirect___fstatat
#define fstatat   __redirect_fstatat
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>
#include <sysdep.h>
#include <time.h>
#include <sys/sysmacros.h>
#include <internal-stat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Lind: route the modern 64-bit-time fstatat through the host runtime.
   The handler fills the buffer with the cage's StatData layout, which
   matches both `struct stat` and `struct __stat64_t64` on wasm32 lind.  */
int
__fstatat64_time64 (int fd, const char *file, struct __stat64_t64 *buf,
		    int flag)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf  = TRANSLATE_GUEST_POINTER_TO_HOST (buf);

  return MAKE_LEGACY_SYSCALL (NEWFSTATAT_SYSCALL, "syscall|fstatat",
			      (uint64_t) fd, host_file, host_buf,
			      (uint64_t) flag, NOTUSED, NOTUSED,
			      TRANSLATE_ERRNO_ON);
}
#if __TIMESIZE != 64
hidden_def (__fstatat64_time64)

int
__fstatat64 (int fd, const char *file, struct stat64 *buf, int flags)
{
  struct __stat64_t64 st_t64;
  return __fstatat64_time64 (fd, file, &st_t64, flags)
	 ?: __cp_stat64_t64_stat64 (&st_t64, buf);
}
#endif

#undef __fstatat
#undef fstatat

hidden_def (__fstatat64)
weak_alias (__fstatat64, fstatat64)

#if XSTAT_IS_XSTAT64
strong_alias (__fstatat64, __fstatat)
weak_alias (__fstatat64, fstatat)
strong_alias (__fstatat64, __GI___fstatat);
#endif
