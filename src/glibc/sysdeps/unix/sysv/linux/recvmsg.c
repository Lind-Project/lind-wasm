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
#include <sys/uio.h>
#include <sysdep-cancel.h>
#include <socketcall.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Lind: translate guest msghdr/iovec pointers to host pointers on the
   stack, following the writev split-pointer pattern.  rawposix just
   casts to libc::msghdr and calls recvmsg directly.  */
static int
__recvmsg_syscall (int fd, struct msghdr *msg, int flags)
{
  int iovcnt = (int) msg->msg_iovlen;

  /* Build host iov array with translated iov_base pointers.  */
  struct iovec host_iov[iovcnt];
  for (int i = 0; i < iovcnt; ++i)
    {
      host_iov[i].iov_len = msg->msg_iov[i].iov_len;

      uint32_t guest_ptr32 = (uint32_t)(uintptr_t) msg->msg_iov[i].iov_base;
      uint64_t host_addr64 = TRANSLATE_GUEST_POINTER_TO_HOST (guest_ptr32);

      uint32_t low32  = (uint32_t)(host_addr64 & 0xFFFFFFFFULL);
      uint32_t high32 = (uint32_t)(host_addr64 >> 32);

      host_iov[i].iov_base   = (void *)(uintptr_t) low32;
      host_iov[i].__padding1 = (int) high32;
      host_iov[i].__padding2 = 0;
    }

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

  host_msg.msg_flags    = 0;
  host_msg.__pad_flags  = 0;

  ssize_t ret = MAKE_LEGACY_SYSCALL (RECVMSG_SYSCALL, "syscall|recvmsg",
				     (uint64_t) fd,
				     (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&host_msg),
				     (uint64_t) flags,
				     NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);

  if (ret >= 0)
    {
      /* Copy back output fields that the kernel updated.  */
      msg->msg_namelen   = host_msg.msg_namelen;
      msg->msg_controllen = host_msg.msg_controllen;
      msg->msg_flags     = host_msg.msg_flags;
    }

  return ret;
}

ssize_t
__libc_recvmsg64 (int fd, struct msghdr *msg, int flags)
{
  ssize_t r;
#if __TIMESIZE != 64
  socklen_t orig_controllen = msg != NULL ? msg->msg_controllen : 0;
#endif

  r = __recvmsg_syscall (fd, msg, flags);

#if __TIMESIZE != 64
  if (r >= 0 && orig_controllen != 0)
    __convert_scm_timestamps (msg, orig_controllen);
#endif

  return r;
}
#if __TIMESIZE != 64
weak_alias (__libc_recvmsg64, __recvmsg64)

ssize_t
__libc_recvmsg (int fd, struct msghdr *msg, int flags)
{
  return __recvmsg_syscall (fd, msg, flags);
}
#endif
weak_alias (__libc_recvmsg, recvmsg)
weak_alias (__libc_recvmsg, __recvmsg)
