/* Get file status.  Linux version.
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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

#include <sys/stat.h>
#include <fcntl.h>
#include <kernel_stat.h>
#include <syscall-template.h>

#if !XSTAT_IS_XSTAT64
int
__stat (const char *fd, struct stat *buf)
{
 return MAKE_SYSCALL(9, "syscall|xstat", (uint64_t) fd, (uint64_t) buf, NOTUSED, NOTUSED, NOTUSED, NOTUSED); 
}

weak_alias (__stat, stat)
#endif
