/* Copyright (C) 1997-2024 Free Software Foundation, Inc.
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
#include <sysdep-cancel.h>
#include <shlib-compat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#ifndef __OFF_T_MATCHES_OFF64_T

// ssize_t
// __libc_pread (int fd, void *buf, size_t count, off_t offset)
// {
//   return MAKE_SYSCALL(21, "syscall|mmap", (uint64_t)(uintptr_t) addr,
//   (uint64_t) len, (uint64_t) prot, (uint64_t) flags, (uint64_t) fd,
//   (uint64_t) offset);
//   // return SYSCALL_CANCEL (pread64, fd, buf, count, SYSCALL_LL_PRW
//   (offset));
// }

// Edit: Dennis
ssize_t
__libc_pread (int fd, void *buf, size_t count, off_t offset)
{
  return MAKE_SYSCALL (PREAD_SYSCALL, "syscall|pread", (uint64_t) fd,
		       TRANSLATE_GUEST_POINTER_TO_HOST (buf), (uint64_t) count,
		       (uint64_t) offset, NOTUSED, NOTUSED);
  // return SYSCALL_CANCEL (pread64, fd, buf, count, SYSCALL_LL_PRW (offset));
}

strong_alias (__libc_pread, __pread) libc_hidden_weak (__pread)
    weak_alias (__libc_pread, pread)

#  if OTHER_SHLIB_COMPAT(libpthread, GLIBC_2_1, GLIBC_2_2)
	compat_symbol (libc, __libc_pread, pread, GLIBC_2_2);
#  endif

#endif
