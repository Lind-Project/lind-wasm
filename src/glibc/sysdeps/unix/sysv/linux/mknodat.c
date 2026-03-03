/* Create a special or ordinary file.  Linux version.
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

#include <sys/types.h>
#include <sys/stat.h>
#include <errno.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>


/* We modify mknodat.c instead of mknod.c because mknod() delegates to
   __mknodat(AT_FDCWD, path, mode, dev) (see io/mknod.c).  The legacy
   compatibility wrappers xmknod.c and xmknodat.c also call __mknodat
   under the hood, so this single change covers all entry points.  */
int
__mknodat (int fd, const char *path, mode_t mode, dev_t dev)
{
  return MAKE_LEGACY_SYSCALL(MKNOD_SYSCALL, "syscall|mknod",
     (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(path), (uint64_t) mode, (uint64_t) dev, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
libc_hidden_def (__mknodat)
weak_alias (__mknodat, mknodat)
