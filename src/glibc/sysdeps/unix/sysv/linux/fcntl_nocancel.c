/* Linux fcntl syscall implementation -- non-cancellable.
   Copyright (C) 2018-2024 Free Software Foundation, Inc.
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

#include <fcntl.h>
#include <stdarg.h>
#include <errno.h>
#include <sysdep-cancel.h>
#include <not-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#ifndef __NR_fcntl64
# define __NR_fcntl64 __NR_fcntl
#endif

#ifndef FCNTL_ADJUST_CMD
# define FCNTL_ADJUST_CMD(__cmd) __cmd
#endif

int
__fcntl64_nocancel (int fd, int cmd, ...)
{
  va_list ap;
  void *arg;

  va_start (ap, cmd);
  arg = va_arg (ap, void *);
  va_end (ap);

  cmd = FCNTL_ADJUST_CMD (cmd);

  return __fcntl64_nocancel_adjusted (fd, cmd, arg);
}
hidden_def (__fcntl64_nocancel)

int
__fcntl64_nocancel_adjusted (int fd, int cmd, void *arg)
{
  if (cmd == F_GETOWN)
    {
      struct f_owner_ex fex;
      int res = MAKE_TRANDITION (
	  FCNTL_SYSCALL, "syscall|fcntl", (uint64_t) fd,
	  (uint64_t) F_GETOWN_EX, NOTUSED,
	  (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&fex), NOTUSED, NOTUSED, WRAPPED_SYSCALL);
      if (!INTERNAL_SYSCALL_ERROR_P (res))
	return fex.type == F_OWNER_GID ? -fex.pid : fex.pid;

      return INLINE_SYSCALL_ERROR_RETURN_VALUE (INTERNAL_SYSCALL_ERRNO (res));
    }

  /* We populate separate slots for integer args and pointer args.
     rawposix uses the appropriate slot based on the command. */
  uint64_t int_arg = 0;
  uint64_t ptr_arg = 0;

  /* Check command type to determine if arg is a pointer or integer */
  if (cmd == F_GETLK || cmd == F_GETLK64 || cmd == F_SETLK || cmd == F_SETLK64
      || cmd == F_SETLKW || cmd == F_SETLKW64)
    {
      /* Lock operation - arg is a struct flock pointer */
      ptr_arg = (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (arg);
      int_arg = 0; /* Unused for pointer commands */
    }
  else
    {
      /* Integer argument (flags, fd numbers, etc.) - no translation */
      int_arg = (uint64_t) (uintptr_t) arg;
      ptr_arg = 0; /* Unused for integer commands */
    }

  return MAKE_TRANDITION (FCNTL_SYSCALL, "syscall|fcntl", (uint64_t) fd,
		       (uint64_t) cmd, int_arg, ptr_arg, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
