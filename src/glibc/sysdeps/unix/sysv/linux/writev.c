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
#include <syscall-template.h>
#include <lind_syscall_num.h>

ssize_t
__writev (int fd, const struct iovec *iov, int iovcnt)
{
  if (iovcnt < 0) {
    __set_errno(EINVAL);
    return -1;
  }

  if (iovcnt == 0) {
    return 0;
  }

  struct iovec *host_iov = malloc(iovcnt * sizeof(struct iovec));
  for (int i = 0; i < iovcnt; ++i) {
    host_iov[i].iov_base = TRANSLATE_GUEST_POINTER_TO_HOST(user_iov[i].iov_base);
    host_iov[i].iov_len  = user_iov[i].iov_len;
  }

  ssize_t ret = MAKE_SYSCALL(WRITEV_SYSCALL, "syscall|writev", (uint64_t) fd, (uint64_t)(uintptr_t) host_iov, (uint64_t) iovcnt, NOTUSED, NOTUSED, NOTUSED);
  free(host_iov);
  return ret;
}
libc_hidden_def (__writev)
weak_alias (__writev, writev)
