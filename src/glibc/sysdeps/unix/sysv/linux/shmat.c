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
#include <addr_translation.h>

/* Attach the shared memory segment associated with SHMID to the data
   segment of the calling process.  SHMADDR and SHMFLG determine how
   and where the segment is attached.  */

void *
shmat (int shmid, const void *shmaddr, int shmflg)
{
  // shmaddr CAN be NULL - kernel chooses the address (like mmap)
  // This is the recommended way to use shmat for portability
  // Do NOT add null check here - NULL is valid and expected
  uint64_t host_shmaddr = TRANSLATE_GUEST_POINTER_TO_HOST (shmaddr);
  return MAKE_LEGACY_SYSCALL (SHMAT_SYSCALL, "syscall|shmat", (uint64_t) shmid,
		       host_shmaddr, (uint64_t) shmflg,
		       NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
