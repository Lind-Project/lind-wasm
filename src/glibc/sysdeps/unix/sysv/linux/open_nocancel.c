/* Linux open syscall implementation, non-LFS, non-cancellable.
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
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#include <sys/types.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <stdarg.h>

#include <sysdep-cancel.h>
#include <not-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#ifndef __OFF_T_MATCHES_OFF64_T

int
__open_nocancel (const char *file, int oflag, ...)
{
  int mode = 0;

  if (__OPEN_NEEDS_MODE (oflag))
    {
      va_list arg;
      va_start (arg, oflag);
      mode = va_arg (arg, int);
      va_end (arg);
    }

  // Added MAKE_SYSCALL macro to interface with Lind - Qianxi Chen
  return MAKE_SYSCALL(OPEN_SYSCALL, "syscall|open", (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(file), (uint64_t) oflag, (uint64_t) mode, NOTUSED, NOTUSED, NOTUSED);
}
hidden_def (__open_nocancel)

#endif
