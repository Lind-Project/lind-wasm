/* Implementation of the getrandom system call.
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
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <sys/random.h>
#include <errno.h>
#include <unistd.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Write up to LENGTH bytes of randomness starting at BUFFER.
   Return the number of bytes written, or -1 on error.  */
ssize_t
__getrandom (void *buffer, size_t length, unsigned int flags)
{
   uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST(buffer);

  return MAKE_LEGACY_SYSCALL (GETRANDOM_SYSCALL, "syscall|getrandom", (uint64_t) host_buf,
		       (uint64_t) length, (uint64_t) flags, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
libc_hidden_def (__getrandom)
weak_alias (__getrandom, getrandom)
