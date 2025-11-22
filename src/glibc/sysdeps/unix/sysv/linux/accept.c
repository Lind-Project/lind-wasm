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

int
__libc_accept (int fd, struct sockaddr *addr, socklen_t *len)
{
  // Dennis Edit
  // NOTE: addr and len can be NULL - this is valid to not get peer address
  // Do NOT add null checks - NULL is valid and common for accept
  uint64_t host_addr = TRANSLATE_GUEST_POINTER_TO_HOST (addr);
  uint64_t host_len = TRANSLATE_GUEST_POINTER_TO_HOST (len);
  
  return MAKE_TRADITION (ACCEPT_SYSCALL, "syscall|accept", (uint64_t) fd,
		       host_addr, host_len,
		       NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
weak_alias (__libc_accept, accept)
libc_hidden_def (accept)
