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
#include <lind_syscall_num.h>
#include <addr_translation.h>
#include <syscall-template.h>

#if __TIMESIZE == 64 \
     && (__WORDSIZE == 32 \
     && (!defined __SYSCALL_WORDSIZE || __SYSCALL_WORDSIZE == 32))
/* Sanity check to avoid newer 32-bit ABI to support non-LFS calls.  */
_Static_assert (sizeof (__off_t) == sizeof (__off64_t),
                "__blkcnt_t and __blkcnt64_t must match");
_Static_assert (sizeof (__ino_t) == sizeof (__ino64_t),
                "__blkcnt_t and __blkcnt64_t must match");
_Static_assert (sizeof (__blkcnt_t) == sizeof (__blkcnt64_t),
                "__blkcnt_t and __blkcnt64_t must match");
#endif

#if FSTATAT_USE_STATX

static inline int
fstatat64_time64_statx (int fd, const char *file, struct __stat64_t64 *buf,
			int flag)
{
  /* lind-wasm: statx is not implemented; route through rawposix fstatat. */
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  /* Use translate_errno=0 so INTERNAL_SYSCALL_ERROR_P sees the raw -errno. */
  int r = MAKE_LEGACY_SYSCALL (FSTATAT_SYSCALL, "syscall|fstatat",
      (uint64_t) fd, host_file, host_buf, (uint64_t) flag,
      NOTUSED, NOTUSED, 0);
  return r;
}
#endif

/* Only statx supports 64-bit timestamps for 32-bit architectures with
   __ASSUME_STATX, so there is no point in building the fallback.  */
#if !FSTATAT_USE_STATX || (FSTATAT_USE_STATX && !defined __ASSUME_STATX)
static inline int
fstatat64_time64_stat (int fd, const char *file, struct __stat64_t64 *buf,
		       int flag)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  /* Use translate_errno=0 so INTERNAL_SYSCALL_ERROR_P sees the raw -errno. */
  int r = MAKE_LEGACY_SYSCALL (FSTATAT_SYSCALL, "syscall|fstatat",
      (uint64_t) fd, host_file, host_buf, (uint64_t) flag,
      NOTUSED, NOTUSED, 0);
  return r;
}
#endif

int
__fstatat64_time64 (int fd, const char *file, struct __stat64_t64 *buf,
		    int flag)
{
  int r;

#if FSTATAT_USE_STATX
  r = fstatat64_time64_statx (fd, file, buf, flag);
# ifndef __ASSUME_STATX
  if (r == -ENOSYS)
    r = fstatat64_time64_stat (fd, file, buf, flag);
# endif
#else
  r = fstatat64_time64_stat (fd, file, buf, flag);
#endif

  return INTERNAL_SYSCALL_ERROR_P (r)
	 ? INLINE_SYSCALL_ERROR_RETURN_VALUE (-r)
	 : 0;
}
#if __TIMESIZE != 64
hidden_def (__fstatat64_time64)

int
__fstatat64 (int fd, const char *file, struct stat64 *buf, int flags)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  return MAKE_LEGACY_SYSCALL (FSTATAT_SYSCALL, "syscall|fstatat",
      (uint64_t) fd, host_file, host_buf, (uint64_t) flags,
      NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
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
