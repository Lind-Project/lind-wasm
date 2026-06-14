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
   Library General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#include <signal.h>
#include <unistd.h>

/* Suspend the process until a signal arrives.
   Implemented as sigsuspend with the current mask so that the
   underlying rt_sigsuspend syscall handles both atomically.  */
int
__libc_pause (void)
{
  sigset_t set;
  __sigprocmask (SIG_BLOCK, NULL, &set);
  return __sigsuspend (&set);
}
weak_alias (__libc_pause, pause)
