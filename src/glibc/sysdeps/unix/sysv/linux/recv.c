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

ssize_t
__libc_recv (int fd, void *buf, size_t len, int flags)
{
  // From the man page: https://man7.org/linux/man-pages/man2/recv.2.html
  // `recv(sockfd, buf, size, flags);`
  // is equivalent to
  // `recv(sockfd, buf, size, flags, NULL, NULL);`
  return MAKE_SYSCALL (RECVFROM_SYSCALL, "syscall|recvfrom", (uint64_t) fd,
		       (uint64_t) buf, (uint64_t) len, (uint64_t) flags,
		       NOTUSED, NOTUSED);
}
weak_alias (__libc_recv, recv) weak_alias (__libc_recv, __recv)
    libc_hidden_weak (__recv)
