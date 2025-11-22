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

#include <sysdep.h>
#include <fcntl.h>
#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__kill (__pid_t a,  int b)
{
   return MAKE_TRADITION(KILL_SYSCALL, "syscall|kill", (uint64_t) a, (uint64_t) b, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

int
kill (__pid_t a,  int b)
{
   return MAKE_TRADITION(KILL_SYSCALL, "syscall|kill", (uint64_t) a, (uint64_t) b, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
