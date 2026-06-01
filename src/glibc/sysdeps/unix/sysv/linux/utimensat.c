/* Change access and modification times of open file.  Linux version.
   Copyright (C) 2007-2024 Free Software Foundation, Inc.
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

#include <errno.h>
#include <sys/stat.h>
#include <sysdep.h>
#include <time.h>
#include <kernel-features.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Helper function defined for easy reusage of the code which calls utimensat.

   Lind-specific: route through MAKE_LEGACY_SYSCALL with translated guest
   pointers so the host (rawposix or a forwarding grate) sees host
   addresses for both `file` and the times buffer.  Both pointers may be
   NULL — `file == NULL` means "operate on `fd` itself" (the futimens
   path); `tsp64 == NULL` means "set both times to now".

   The cage's `struct __timespec64` is `{ i64 tv_sec; i32 tv_nsec; i32
   _pad; }` (long is 32-bit on wasm32).  The Linux kernel utimensat
   syscall expects `struct __kernel_timespec` = `{ i64 tv_sec; i64
   tv_nsec; }` — tv_nsec spanning 8 bytes.  Passing the cage pointer
   verbatim would have the host read 4 bytes of stack-uninitialized
   __padding as the high half of tv_nsec → garbage out-of-range nsec,
   kernel returns EINVAL.  Copy into a kernel-ABI buffer here so the
   host syscall path sees the layout it expects.  */
int
__utimensat64_helper (int fd, const char *file,
                      const struct __timespec64 tsp64[2], int flags)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_tsp;

  struct { int64_t tv_sec; int64_t tv_nsec; } kts[2];
  if (tsp64 != NULL)
    {
      kts[0].tv_sec  = tsp64[0].tv_sec;
      kts[0].tv_nsec = tsp64[0].tv_nsec;
      kts[1].tv_sec  = tsp64[1].tv_sec;
      kts[1].tv_nsec = tsp64[1].tv_nsec;
      host_tsp = TRANSLATE_GUEST_POINTER_TO_HOST (kts);
    }
  else
    host_tsp = 0;

  return MAKE_LEGACY_SYSCALL (UTIMENSAT_SYSCALL, "syscall|utimensat",
			      (uint64_t) fd, host_file, host_tsp,
			      (uint64_t) flags, NOTUSED, NOTUSED,
			      TRANSLATE_ERRNO_ON);
}
libc_hidden_def (__utimensat64_helper)

/* Change the access time of FILE to TSP[0] and
   the modification time of FILE to TSP[1].

   Starting with 2.6.22 the Linux kernel has the utimensat syscall.  */
int
__utimensat64 (int fd, const char *file, const struct __timespec64 tsp64[2],
               int flags)
{
  if (file == NULL)
    return INLINE_SYSCALL_ERROR_RETURN_VALUE (EINVAL);

  return __utimensat64_helper (fd, file, &tsp64[0], flags);
}

#if __TIMESIZE != 64
libc_hidden_def (__utimensat64)

int
__utimensat (int fd, const char *file, const struct timespec tsp[2],
             int flags)
{
  struct __timespec64 tsp64[2];
  if (tsp)
    {
      tsp64[0] = valid_timespec_to_timespec64 (tsp[0]);
      tsp64[1] = valid_timespec_to_timespec64 (tsp[1]);
    }

  return __utimensat64 (fd, file, tsp ? &tsp64[0] : NULL, flags);
}
#endif
weak_alias (__utimensat, utimensat)
