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
  struct iovec host_iov[iovcnt];
  for (int i = 0; i < iovcnt; ++i)
  {
    host_iov[i].iov_len = iov[i].iov_len;
    // Translate to a 64-bit host pointer
    uint32_t guest_ptr32 = (uint32_t)(uintptr_t) iov[i].iov_base;
    uint64_t host_addr64 = TRANSLATE_GUEST_POINTER_TO_HOST(guest_ptr32);

    // Split host_addr64 into low32 / high32
    uint32_t low32  = (uint32_t)(host_addr64 & 0xFFFFFFFFULL);
    uint32_t high32 = (uint32_t)(host_addr64 >> 32);

    // Store lower and higher bits into padded iov struct
    host_iov[i].iov_base   = (void*)(uintptr_t)low32;
    host_iov[i].__padding1 = (int)high32;
    host_iov[i].__padding2 = 0;
  }

  ssize_t ret = MAKE_LEGACY_SYSCALL (WRITEV_SYSCALL, "syscall|writev", (uint64_t) fd,
			      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST((uintptr_t) host_iov),
			      (uint64_t) iovcnt, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
  return ret;
}
libc_hidden_def (__writev)
weak_alias (__writev, writev)
