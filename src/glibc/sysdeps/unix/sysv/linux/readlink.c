/* Read value of a symbolic link.  Linux version.
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
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <errno.h>

/* Read the contents of the symbolic link PATH into no more than
   LEN bytes of BUF.  The contents are not null-terminated.
   Returns the number of characters read, or -1 for errors.  */
/*
* Edit Note:
* Linux kernel has two different implementations for `readlink` and `readlinkat` syscall.
* In original glibc implementaion, there was only one entry point and `readlinkat` will be redirected through `readlink`, 
* and kernel has different callnums for them so in lind-wasm, we have separately implemented `readlink` and `readlinkat`.
*/
ssize_t
__readlink (const char *path, char *buf, size_t len)
{
  return MAKE_SYSCALL(165, "syscall|readlink", (uint64_t) path, (uint64_t)(uintptr_t) buf, (uint64_t) len, NOTUSED, NOTUSED, NOTUSED);
}
weak_alias (__readlink, readlink)
