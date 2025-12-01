/* Change permissions of a file.  Linux version.
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

#include <fcntl.h>
#include <sysdep.h>
#include <unistd.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Change the protections of FILE to MODE.  */
int
__chmod (const char *file, mode_t mode)
{
  return MAKE_LEGACY_SYSCALL (CHMOD_SYSCALL, "syscall|chmod",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (file),
		       (uint64_t) mode, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

libc_hidden_def (__chmod)
weak_alias (__chmod, chmod)
