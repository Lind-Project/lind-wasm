/* Copyright (C) 1995-2024 Free Software Foundation, Inc.
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

#include <ipc_priv.h>
#include <sysdep.h>
#include <errno.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

/* Detach shared memory segment starting at address specified by SHMADDR
   from the caller's data segment.  */

int
shmdt (const void *shmaddr)
{
   return MAKE_SYSCALL(SHMDT_SYSCALL, "syscall|shmdt", (uint64_t) shmaddr, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
