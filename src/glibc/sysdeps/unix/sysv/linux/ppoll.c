/* Copyright (C) 2006-2024 Free Software Foundation, Inc.
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
#include <signal.h>
#include <time.h>
#include <sys/poll.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__ppoll64 (struct pollfd *fds, nfds_t nfds, const struct __timespec64 *timeout,
           const sigset_t *sigmask)
{
  /* Convert timespec to milliseconds.
     NULL timeout means block indefinitely (-1). */
  int timeout_ms;
  if (timeout == NULL)
    {
      timeout_ms = -1;
    }
  else
    {
      timeout_ms = (int)(timeout->tv_sec * 1000 + timeout->tv_nsec / 1000000);
      if (timeout_ms < 0)
        timeout_ms = 0;
    }

  uint64_t host_fds = TRANSLATE_GUEST_POINTER_TO_HOST (fds);
  uint64_t host_sigmask = sigmask
    ? (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (sigmask)
    : 0;

  return MAKE_LEGACY_SYSCALL (PPOLL_SYSCALL, "syscall|ppoll",
               host_fds, (uint64_t) nfds, (uint64_t) timeout_ms,
               host_sigmask, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

#if __TIMESIZE != 64
libc_hidden_def (__ppoll64)

int
ppoll (struct pollfd *fds, nfds_t nfds, const struct timespec *timeout,
         const sigset_t *sigmask)
{
  struct __timespec64 ts64;
  if (timeout)
    ts64 = valid_timespec_to_timespec64 (*timeout);

  return __ppoll64 (fds, nfds, timeout ? &ts64 : NULL, sigmask);
}
#endif
libc_hidden_def (ppoll)
