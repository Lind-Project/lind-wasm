/* Compatibility implementation of sendmsg.
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
#include <sys/uio.h>
#include <sysdep-cancel.h>
#include <socketcall.h>
#include <shlib-compat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

ssize_t
__libc_sendmsg (int fd, const struct msghdr *msg, int flags)
{
  int iovcnt = (int) msg->msg_iovlen;

  /* Build host iov array with translated iov_base pointers.  */
  struct iovec host_iov[iovcnt];
  __lind_translate_iov (msg->msg_iov, host_iov, iovcnt);

  /* Build host msghdr with translated pointers using split-pointer trick.  */
  struct msghdr host_msg;
  uint64_t addr;

  /* msg_name */
  addr = TRANSLATE_GUEST_POINTER_TO_HOST (msg->msg_name);
  host_msg.msg_name      = (void *)(uintptr_t)(uint32_t)(addr & 0xFFFFFFFFULL);
  host_msg.__pad_name    = (int)(uint32_t)(addr >> 32);
  host_msg.msg_namelen   = msg->msg_namelen;
  host_msg.__pad_namelen = 0;

  /* msg_iov — point to translated host_iov array */
  addr = TRANSLATE_GUEST_POINTER_TO_HOST (host_iov);
  host_msg.msg_iov      = (struct iovec *)(uintptr_t)(uint32_t)(addr & 0xFFFFFFFFULL);
  host_msg.__pad_iov    = (int)(uint32_t)(addr >> 32);
  host_msg.msg_iovlen   = msg->msg_iovlen;
  host_msg.__pad_iovlen = 0;

  /* msg_control */
  addr = TRANSLATE_GUEST_POINTER_TO_HOST (msg->msg_control);
  host_msg.msg_control      = (void *)(uintptr_t)(uint32_t)(addr & 0xFFFFFFFFULL);
  host_msg.__pad_control    = (int)(uint32_t)(addr >> 32);
  host_msg.msg_controllen   = msg->msg_controllen;
  host_msg.__pad_controllen = 0;

  host_msg.msg_flags    = msg->msg_flags;
  host_msg.__pad_flags  = 0;

  ssize_t ret = MAKE_LEGACY_SYSCALL (SENDMSG_SYSCALL, "syscall|sendmsg",
				     (uint64_t) fd,
				     (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&host_msg),
				     (uint64_t) flags,
				     NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
  return ret;
}
weak_alias (__libc_sendmsg, sendmsg)
weak_alias (__libc_sendmsg, __sendmsg)
#if __TIMESIZE != 64
weak_alias (__sendmsg, __sendmsg64)
#endif
