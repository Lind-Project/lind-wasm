/* Linux pread64() syscall implementation -- non-cancellable.
   Copyright (C) 2019-2024 Free Software Foundation, Inc.
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
   <http://www.gnu.org/licenses/>.  */

#include <unistd.h>
#include <sysdep-cancel.h>
#include <not-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

ssize_t
__pread64_nocancel (int fd, void *buf, size_t count, off64_t offset)
{
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);

  return MAKE_LEGACY_SYSCALL(PREAD_SYSCALL, "syscall|pread", (uint64_t) fd, host_buf, (uint64_t) count, (uint64_t) offset, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
hidden_def (__pread64_nocancel)
