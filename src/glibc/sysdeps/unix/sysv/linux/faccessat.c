/* Test for access to file, relative to open directory.  Linux version.
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

#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Lind: route faccessat through 3i to RawPOSIX's faccessat_syscall.
   The kernel faccessat doesn't accept any flags except 0; the userspace
   AT_EACCESS / AT_SYMLINK_NOFOLLOW emulation done in upstream glibc via
   stat+euid is left to the host's libc::faccessat.  */
int
__faccessat (int fd, const char *file, int mode, int flag)
{
  if (flag & ~(AT_SYMLINK_NOFOLLOW | AT_EACCESS))
    return INLINE_SYSCALL_ERROR_RETURN_VALUE (EINVAL);

  return MAKE_LEGACY_SYSCALL (FACCESSAT_SYSCALL, "syscall|faccessat",
			      (uint64_t) fd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (file),
			      (uint64_t) mode,
			      (uint64_t) flag,
			      NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias (__faccessat, faccessat)
