/* Manipulate file descriptor.  Linux LFS version.
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

#define fcntl __no_decl_fcntl
#define __fcntl __no_decl___fcntl
#include <fcntl.h>
#undef fcntl
#undef __fcntl
#include <stdarg.h>
#include <errno.h>
#include <sysdep-cancel.h>
#include <lind_syscall_num.h>


#ifndef __NR_fcntl64
# define __NR_fcntl64 __NR_fcntl
#endif

#ifndef FCNTL_ADJUST_CMD
# define FCNTL_ADJUST_CMD(__cmd) __cmd
#endif

#include <syscall-template.h>
#include <addr_translation.h>

int
__libc_fcntl64 (int fd, int cmd, ...)
{
  va_list ap;
  void *arg;

  va_start (ap, cmd);
  arg = va_arg (ap, void *);
  va_end (ap);

  cmd = FCNTL_ADJUST_CMD (cmd);

  if (cmd == F_GETOWN)
    {
      struct f_owner_ex fex;
      int res = MAKE_LEGACY_SYSCALL (
          FCNTL_SYSCALL, "syscall|fcntl", (uint64_t) fd,
          (uint64_t) F_GETOWN_EX, NOTUSED,
          (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&fex),
          NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
      if (!INTERNAL_SYSCALL_ERROR_P (res))
        return fex.type == F_OWNER_GID ? -fex.pid : fex.pid;
      return INLINE_SYSCALL_ERROR_RETURN_VALUE (INTERNAL_SYSCALL_ERRNO (res));
    }

  /* Polymorphic third arg: lock cmds take a pointer (translate it),
     everything else takes an int (do NOT translate; that would add the
     wasm linear-memory base to the integer value).  Mirrors the split
     in __fcntl64_nocancel_adjusted; rawposix reads whichever slot the
     command expects.  */
  uint64_t int_arg = 0;
  uint64_t ptr_arg = 0;

  if (cmd == F_GETLK || cmd == F_GETLK64
      || cmd == F_SETLK || cmd == F_SETLK64
      || cmd == F_SETLKW || cmd == F_SETLKW64
      || cmd == F_OFD_GETLK || cmd == F_OFD_SETLK || cmd == F_OFD_SETLKW)
    {
      ptr_arg = (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (arg);
    }
  else
    {
      int_arg = (uint64_t) (uintptr_t) arg;
    }

  return MAKE_LEGACY_SYSCALL (FCNTL_SYSCALL, "syscall|fcntl",
                              (uint64_t) fd, (uint64_t) cmd,
                              int_arg, ptr_arg, NOTUSED, NOTUSED,
                              TRANSLATE_ERRNO_ON);
}
libc_hidden_def (__libc_fcntl64)
weak_alias (__libc_fcntl64, __fcntl64)
libc_hidden_weak (__fcntl64)
weak_alias (__libc_fcntl64, fcntl64)
#if __TIMESIZE != 64
weak_alias (__libc_fcntl64, __fcntl_time64)
#endif

#ifdef __OFF_T_MATCHES_OFF64_T
weak_alias (__libc_fcntl64, __libc_fcntl)
weak_alias (__libc_fcntl64, __fcntl)
weak_alias (__libc_fcntl64, __GI___fcntl)
weak_alias (__libc_fcntl64, fcntl)
#endif
