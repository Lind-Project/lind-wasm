/* Open an epoll file descriptor.  Linux version.
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

#include <sys/epoll.h>
#include <sysdep.h>
#include <syscall-template.h>

libc_hidden_proto (epoll_ctl)

int
epoll_ctl (int __epfd, int __op, int __fd,
		      struct epoll_event *__event)
{
   return MAKE_SYSCALL(57, "syscall|epoll_ctl", (uint64_t) __epfd, (uint64_t) __op, (uint64_t) __fd, (uint64_t) __event, NOTUSED, NOTUSED);
}
libc_hidden_def (epoll_ctl)
