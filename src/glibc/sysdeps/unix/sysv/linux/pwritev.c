/* Copyright (C) 2009-2024 Free Software Foundation, Inc.
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

#include <sys/uio.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#ifndef __OFF_T_MATCHES_OFF64_T

ssize_t
pwritev (int fd, const struct iovec *vector, int count, off_t offset)
{
  struct iovec host_iov[count];
  __lind_translate_iov (vector, host_iov, count);

  return MAKE_LEGACY_SYSCALL (PWRITEV_SYSCALL, "syscall|pwritev",
               (uint64_t) fd,
               (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST ((uintptr_t) host_iov),
               (uint64_t) count,
               (uint64_t) offset, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
libc_hidden_def (pwritev)
#endif
