/* Determine current working directory.  Linux version.
   Copyright (C) 1997-2024 Free Software Foundation, Inc.
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

#include <assert.h>
#include <errno.h>
#include <limits.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/param.h>

#include <sysdep.h>
#include <sys/syscall.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* If we compile the file for use in ld.so we don't need the feature
   that getcwd() allocates the buffers itself.  */
#if IS_IN (rtld)
# define NO_ALLOCATION	1
#endif

/* The "proc" filesystem provides an easy method to retrieve the value.
   For each process, the corresponding directory contains a symbolic link
   named `cwd'.  Reading the content of this link immediate gives us the
   information.  But we have to take care for systems which do not have
   the proc filesystem mounted.  Use the POSIX implementation in this case.  */

/* Get the code for the generic version.  */
#define GETCWD_RETURN_TYPE	static char *
#include <sysdeps/posix/getcwd.c>

char *
__getcwd (char *buf, size_t size)
{
  // buf CAN be NULL - this means kernel should allocate the buffer
  // Do NOT add null check here - NULL is valid
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  
  return MAKE_SYSCALL (GETCWD_SYSCALL, "syscall|getcwd",
		       host_buf, (uint64_t) size, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
libc_hidden_def (__getcwd)
weak_alias (__getcwd, getcwd)
