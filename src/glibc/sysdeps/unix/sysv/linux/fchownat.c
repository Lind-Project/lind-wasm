/* Change ownership of a file relative to open directory.  Linux version.
   Copyright (C) 2024 Free Software Foundation, Inc.
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
#include <stddef.h>
#include <unistd.h>
#include <sys/types.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Lind: route fchownat through 3i to RawPOSIX's fchownat_syscall.  */
int
fchownat (int fd, const char *file, uid_t owner, gid_t group, int flag)
{
  if (file == NULL || (flag & ~AT_SYMLINK_NOFOLLOW) != 0)
    {
      __set_errno (EINVAL);
      return -1;
    }

  if (fd < 0 && fd != AT_FDCWD)
    {
      __set_errno (EBADF);
      return -1;
    }

  return MAKE_LEGACY_SYSCALL (FCHOWNAT_SYSCALL, "syscall|fchownat",
			      (uint64_t) fd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (file),
			      (uint64_t) owner,
			      (uint64_t) group,
			      (uint64_t) flag,
			      NOTUSED, TRANSLATE_ERRNO_ON);
}
libc_hidden_def (fchownat)
