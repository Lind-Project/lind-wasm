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

#include <sys/msg.h>
#include <stddef.h>
#include <ipc_priv.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

/* Return an identifier for an shared memory segment of at least size SIZE
   which is associated with KEY.  */

int
shmget (key_t key, size_t size, int shmflg)
{
   return MAKE_SYSCALL(SHMGET_SYSCALL, "syscall|shmget", (uint64_t) key, (uint64_t) size, (uint64_t) shmflg, NOTUSED, NOTUSED, NOTUSED);
}
