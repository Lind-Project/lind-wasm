/* Copyright (C) 2008-2024 Free Software Foundation, Inc.
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

#include <errno.h>
#include <signal.h>
#include <sys/socket.h>

#include <sysdep-cancel.h>
#include <sys/syscall.h>
#include <socketcall.h>
#include <kernel-features.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
accept4 (int fd, __SOCKADDR_ARG addr, socklen_t *addr_len, int flags)
{
  uint64_t host_addr = TRANSLATE_GUEST_POINTER_TO_HOST (addr);
  uint64_t host_len = TRANSLATE_GUEST_POINTER_TO_HOST (len);

  return MAKE_LEGACY_SYSCALL(ACCEPT4_SYSCALL, "syscall|accept4",
      (uint64_t) fd, host_addr, host_len,
      (uint64_t) flags, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);

}
