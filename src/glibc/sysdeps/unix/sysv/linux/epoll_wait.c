/* Linux epoll_wait syscall implementation.
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
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#include <stddef.h>
#include <unistd.h>
#include <sys/types.h>
#include <sys/epoll.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
epoll_wait (int epfd, struct epoll_event *events, int maxevents, int timeout)
{
  return MAKE_SYSCALL (
      EPOLL_WAIT_SYSCALL, "syscall|epoll_wait", (uint64_t) epfd,
      (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (events),
      (uint64_t) maxevents, (uint64_t) timeout, NOTUSED, NOTUSED);
}
