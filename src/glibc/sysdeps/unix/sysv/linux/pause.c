/* Linux pause syscall implementation.
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

#include <signal.h>
#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

/* Suspend the process until a signal arrives.
   This always returns -1 and sets errno to EINTR.  */
int
__libc_pause (void)
{
  return MAKE_LEGACY_SYSCALL(PAUSE_SYSCALL, "syscall|pause",
                             NOTUSED, NOTUSED, NOTUSED,
                             NOTUSED, NOTUSED, NOTUSED,
                             TRANSLATE_ERRNO_ON);
}
weak_alias (__libc_pause, pause)
