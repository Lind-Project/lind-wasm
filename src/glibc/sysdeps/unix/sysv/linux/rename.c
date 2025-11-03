/* Linux implementation for rename function.
   Copyright (C) 2016-2024 Free Software Foundation, Inc.
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

#include <stdio.h>
#include <fcntl.h>
#include <sysdep.h>
#include <errno.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Rename the file OLD to NEW.  */
int
rename (const char *old, const char *new)
{
   uint64_t host_old = TRANSLATE_GUEST_POINTER_TO_HOST (old);
   uint64_t host_new = TRANSLATE_GUEST_POINTER_TO_HOST (new);
   
   return MAKE_SYSCALL(RENAME_SYSCALL, "syscall|rename", host_old, host_new, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
