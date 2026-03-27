/* Linux openat syscall implementation, non-LFS.
   Copyright (C) 2005-2024 Free Software Foundation, Inc.
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
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#include <sysdep-cancel.h>

#ifndef __OFF_T_MATCHES_OFF64_T

/* Open FILE with access OFLAG.  Interpret relative paths relative to
   the directory associated with FD.  If OFLAG includes O_CREAT or
   O_TMPFILE, a fourth argument is the file protection.  */
int
__libc_openat (int fd, const char *file, int oflag, ...)
{
  mode_t mode = 0;
  if (__OPEN_NEEDS_MODE (oflag))
    {
      va_list arg;
      va_start (arg, oflag);
      mode = va_arg (arg, mode_t);
      va_end (arg);
    }
  
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);

  return MAKE_LEGACY_SYSCALL (
      OPENAT_SYSCALL, "syscall|openat", (uint64_t) fd, host_file,
      (uint64_t) oflag, (uint64_t) mode, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias (__libc_openat, __openat)
libc_hidden_weak (__openat)
weak_alias (__libc_openat, openat)
#endif
