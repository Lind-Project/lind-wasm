/* Linux writev syscall implementation.
   Copyright (C) 2017-2024 Free Software Foundation, Inc.
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

#include <unistd.h>
#include <sys/uio.h>
#include <sysdep-cancel.h>
#include <addr_translation.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <errno.h>
#include <stdlib.h>

ssize_t
__writev (int fd, const struct iovec *iov, int iovcnt)
{
  struct iovec *host_iov = (struct iovec *) iov;
  for (int i = 0; i < iovcnt; ++i)
  {
    host_iov[i].iov_base = TRANSLATE_GUEST_POINTER_TO_HOST (host_iov[i].iov_base);
  }

  ssize_t ret = MAKE_LEGACY_SYSCALL (WRITEV_SYSCALL, "syscall|writev", (uint64_t) fd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(host_iov),
			      (uint64_t) iovcnt, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
  return ret;
}
libc_hidden_def (__writev)
weak_alias (__writev, writev)
