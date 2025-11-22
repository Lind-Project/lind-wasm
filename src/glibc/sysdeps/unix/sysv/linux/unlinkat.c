/* Delete a name and possibly the file it refers to in a directory. Linux version.
   Copyright (C) 2011-2024 Free Software Foundation, Inc.
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

#include <unistd.h>
#include <fcntl.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Remove the link named NAME in the directory referred to by DIRFD, using FLAGS.  */
int
__unlinkat (int dirfd, const char *name, int flags)
{
   uint64_t host_name = TRANSLATE_GUEST_POINTER_TO_HOST (name);
   
   return MAKE_TRANDITION(UNLINKAT_SYSCALL, "syscall|unlinkat", (uint64_t) dirfd, host_name, (uint64_t) flags, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

weak_alias (__unlinkat, unlinkat)
