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

/* Lind: route via NEWFSTATAT_SYSCALL so we get true lstat (don't-follow-symlink)
   semantics.  Previously this used XSTAT_SYSCALL (regular stat), which followed
   symlinks — wrong, but unavoidable until fstatat existed in rawposix.  */
int
__lstat64_time64 (const char *file, struct __stat64_t64 *buf)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  return MAKE_LEGACY_SYSCALL (NEWFSTATAT_SYSCALL, "syscall|fstatat",
		       (uint64_t) AT_FDCWD, host_file, host_buf,
		       (uint64_t) AT_SYMLINK_NOFOLLOW,
		       NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
#if __TIMESIZE != 64
hidden_def (__lstat64_time64)

int
__lstat64 (const char *file, struct stat64 *buf)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  return MAKE_LEGACY_SYSCALL (NEWFSTATAT_SYSCALL, "syscall|fstatat",
		       (uint64_t) AT_FDCWD, host_file, host_buf,
		       (uint64_t) AT_SYMLINK_NOFOLLOW,
		       NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
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
