/* Selective file content synch'ing.
   Copyright (C) 2006-2024 Free Software Foundation, Inc.
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

#include <fcntl.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
sync_file_range (int fd, __off64_t offset, __off64_t len, unsigned int flags)
{
   return MAKE_LEGACY_SYSCALL(SYNC_FILE_RANGE, "syscall|sync_file_range", (uint64_t) fd, (uint64_t) offset, (uint64_t) len, (uint64_t) flags, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
