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
#include <stdint.h>

int
__ioctl (int fd, unsigned long int request, ...)
{
  va_list args;
  va_start (args, request);

  /* Use unsigned long to safely capture either pointer or integer values */
  unsigned long raw = va_arg (args, unsigned long);
  va_end (args);
  
  /* For FIONBIO and FIOASYNC, the third argument should be a pointer to int
   * (int *argp). Translate the guest pointer to host pointer.
   * TODO: Handle the edge case where someone passes a direct integer value (0
   * or 1) instead of a pointer. For now, we assume correct API usage
   * (pointer). */
  uint64_t host_ptr
      = TRANSLATE_GUEST_POINTER_TO_HOST ((void *) (uintptr_t) raw);

  return MAKE_LEGACY_SYSCALL (IOCTL_SYSCALL, "syscall|ioctl", (uint64_t) fd,
		       (uint64_t) request, host_ptr, NOTUSED, NOTUSED,
		       NOTUSED, WRAPPED_SYSCALL);
}
libc_hidden_def (__ioctl)
weak_alias (__ioctl, ioctl)

#if __TIMESIZE != 64
strong_alias (__ioctl, __ioctl_time64)
#endif
