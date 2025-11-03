/* Copyright (C) 2015-2024 Free Software Foundation, Inc.
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

#include <sys/socket.h>
#include <sysdep-cancel.h>
#include <socketcall.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

ssize_t
__libc_send (int fd, const void *buf, size_t len, int flags)
{
  // From the man page: https://man7.org/linux/man-pages/man2/send.2.html
  // `send(sockfd, buf, size, flags);`
  // is equivalent to
  // `sendto(sockfd, buf, size, flags, NULL, 0);`
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  
  return MAKE_SYSCALL (SENDTO_SYSCALL, "syscall|sendto", (uint64_t) fd,
		       host_buf, (uint64_t) len, (uint64_t) flags, 0, 0);
}
weak_alias (__libc_send, send)
weak_alias (__libc_send, __send)
libc_hidden_def (__send)
