/* Control device.  Linux generic implementation.
   Copyright (C) 2021-2024 Free Software Foundation, Inc.
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

#include <stdarg.h>
#include <sys/ioctl.h>
#include <sysdep.h>
#include <internal-ioctl.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>
#include <assert.h>
#include <stdint.h>

int
__ioctl (int fd, unsigned long int request, ...)
{
  va_list args;
  va_start (args, request);

  /* Use unsigned long to safely capture either pointer or integer values */
  unsigned long raw = va_arg (args, unsigned long);
  va_end (args);

  /* Only support FIONBIO and FIOASYNC.  Fail fast otherwise. */
  assert (request == FIONBIO || request == FIOASYNC);

  uintptr_t third_arg;
  if (raw == 0 || raw == 1)
    {
      int tmp = (int) raw;

      /* Directly use &tmp as the pointer, since it's a host stack variable. */
      third_arg = (uintptr_t) &tmp;

      return MAKE_SYSCALL (IOCTL_SYSCALL, "syscall|ioctl",
                           (uint64_t) fd, (uint64_t) request,
                           (uint64_t) third_arg,
                           NOTUSED, NOTUSED, NOTUSED);
    }
  else
    {
      void *host_ptr = TRANSLATE_GUEST_POINTER_TO_HOST ((void *) (uintptr_t) raw);
      if (host_ptr == NULL)
        {
          errno = EFAULT;
          return -1;
        }

      third_arg = (uintptr_t) host_ptr;

      return MAKE_SYSCALL (IOCTL_SYSCALL, "syscall|ioctl",
                           (uint64_t) fd, (uint64_t) request,
                           (uint64_t) third_arg,
                           NOTUSED, NOTUSED, NOTUSED);
    }
}

libc_hidden_def (__ioctl)
weak_alias (__ioctl, ioctl)

#if __TIMESIZE != 64
strong_alias (__ioctl, __ioctl_time64)
#endif
