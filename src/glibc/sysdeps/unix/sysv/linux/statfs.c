/* Copyright (C) 2011-2024 Free Software Foundation, Inc.
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

#include <sys/statfs.h>
#include <time.h>
#include <sysdep.h>
#include <kernel_stat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#if !STATFS_IS_STATFS64

/* Return information about the filesystem on which FILE resides.  */
int
__statfs (const char *file, struct statfs *buf)
{
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  return MAKE_SYSCALL (STATFS_SYSCALL, "syscall|statfs",
		       host_file, host_buf,
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
libc_hidden_def (__statfs)
weak_alias (__statfs, statfs)
#endif
