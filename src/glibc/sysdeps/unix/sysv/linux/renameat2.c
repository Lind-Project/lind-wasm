/* Linux implementation for renameat2 function.
   Copyright (C) 2018-2024 Free Software Foundation, Inc.
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

#include <errno.h>
#include <stdio.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Lind: route renameat2 through 3i to RawPOSIX's renameat2_syscall.  */
int
__renameat2 (int oldfd, const char *old, int newfd, const char *new,
           unsigned int flags)
{
  return MAKE_LEGACY_SYSCALL (RENAMEAT2_SYSCALL, "syscall|renameat2",
			      (uint64_t) oldfd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (old),
			      (uint64_t) newfd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (new),
			      (uint64_t) flags, NOTUSED, TRANSLATE_ERRNO_ON);
}
libc_hidden_def (__renameat2)
weak_alias (__renameat2, renameat2)
