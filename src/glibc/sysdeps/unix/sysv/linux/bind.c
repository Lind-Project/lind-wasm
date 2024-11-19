/* Copyright (C) 2015-2024 Free Software Foundation, Inc.
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

#include <sys/socket.h>
#include <socketcall.h>
#include <syscall-template.h>

int
__bind (int fd, const struct sockaddr * addr, socklen_t len)
{
  // Dennis Edit
  return MAKE_SYSCALL(33, "syscall|bind", (uint64_t) fd, (uint64_t)(uintptr_t) addr, (uint64_t) len, NOTUSED, NOTUSED, NOTUSED);
}
weak_alias (__bind, bind)
