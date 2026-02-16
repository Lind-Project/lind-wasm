/* Linux recvmsg syscall wrapper.
   Copyright (C) 2016-2024 Free Software Foundation, Inc.
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

/* Lind: use legacy syscall 47 so rawposix recvmsg_syscall can translate
   guest msghdr/iovec and perform host recvmsg.  */
ssize_t
__libc_recvmsg (int fd, struct msghdr *msg, int flags)
{
  return MAKE_LEGACY_SYSCALL (RECVMSG_SYSCALL, "syscall|recvmsg", (uint64_t) fd,
			      (uint64_t)(uintptr_t) msg, (uint64_t) flags,
			      NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias (__libc_recvmsg, recvmsg)
weak_alias (__libc_recvmsg, __recvmsg)
