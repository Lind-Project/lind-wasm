/* Linux select implementation.
   Copyright (C) 2017-2024 Free Software Foundation, Inc.
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

#include <sys/time.h>
#include <sys/types.h>
#include <sys/select.h>
#include <errno.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Check the first NFDS descriptors each in READFDS (if not NULL) for read
   readiness, in WRITEFDS (if not NULL) for write readiness, and in EXCEPTFDS
   (if not NULL) for exceptional conditions.  If TIMEOUT is not NULL, time out
   after waiting the interval specified therein.  Returns the number of ready
   descriptors, or -1 for errors.  */

int
__select64 (int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
	    struct __timeval64 *timeout)
{
  return MAKE_TRANDITION (SELECT_SYSCALL, "syscall|select", (uint64_t) nfds,
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (readfds),
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (writefds),
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (exceptfds),
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (timeout),
		       NOTUSED, WRAPPED_SYSCALL);

  // Lind-Wasm: Original glibc code removed for compatibility
  // to find original source code refer to (2.39.9000) at
  // (/home/lind-wasm/glibc/sysdeps/unix/sysv/linux/select.c):(35-138)
}

#if __TIMESIZE != 64
libc_hidden_def (__select64)

int
__select (int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
	  struct timeval *timeout)
{
  struct __timeval64 tv64, *ptv64 = NULL;
  if (timeout != NULL)
    {
      tv64 = valid_timeval_to_timeval64 (*timeout);
      ptv64 = &tv64;
    }
  int r = __select64 (nfds, readfds, writefds, exceptfds, ptv64);
  if (timeout != NULL)
    /* The remanining timeout will be always less the input TIMEOUT.  */
    *timeout = valid_timeval64_to_timeval (tv64);
  return r;
}
#endif
libc_hidden_def (__select)

weak_alias (__select, select)
weak_alias (__select, __libc_select)
