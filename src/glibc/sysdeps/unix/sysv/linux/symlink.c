/* Make a new name for a file.  Linux version.
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

#include <errno.h>
#include <unistd.h>
#include <fcntl.h>

#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__symlink (const char *from, const char *to)
{
  uint64_t host_from = TRANSLATE_GUEST_POINTER_TO_HOST(from);
  uint64_t host_to = TRANSLATE_GUEST_POINTER_TO_HOST(to);

  return MAKE_LEGACY_SYSCALL(SYMLINK_SYSCALL, "syscall|symlink",
      host_from, host_to, NOTUSED,
      NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias (__symlink, symlink)

