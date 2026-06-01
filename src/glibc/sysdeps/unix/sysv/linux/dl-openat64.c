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
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Lind: route the dynamic-loader's openat64 through the same
   OPENAT_SYSCALL handler the rest of the libc uses, instead of the
   broken INLINE_SYSCALL stub.  */
int
openat64 (int dfd, const char *file, int oflag, ...)
{
  assert (!__OPEN_NEEDS_MODE (oflag));

  return MAKE_LEGACY_SYSCALL (OPENAT_SYSCALL, "syscall|openat",
			      (uint64_t) dfd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (file),
			      (uint64_t) (oflag | O_LARGEFILE),
			      0, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
