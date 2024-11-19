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
__socketpair (int domain, int type, int protocol, int sv[2])
{
   return MAKE_SYSCALL(49, "syscall|socketpair", (uint64_t) domain, (uint64_t) type, (uint64_t) protocol, (uint64_t) sv, NOTUSED, NOTUSED);
}
weak_alias (__socketpair, socketpair)
