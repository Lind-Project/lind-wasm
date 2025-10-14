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

ssize_t
__libc_pwrite64 (int fd, const void *buf, size_t count, off64_t offset)
{
  return MAKE_SYSCALL(PWRITE_SYSCALL, "syscall|pwrite", (uint64_t) fd, (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(buf), (uint64_t) count, (uint64_t) offset, NOTUSED, NOTUSED);
}

weak_alias (__libc_pwrite64, __pwrite64)
libc_hidden_weak (__pwrite64)
weak_alias (__libc_pwrite64, pwrite64)

#ifdef __OFF_T_MATCHES_OFF64_T
strong_alias (__libc_pwrite64, __libc_pwrite)
weak_alias (__libc_pwrite64, __pwrite)
weak_alias (__libc_pwrite64, pwrite)

# if OTHER_SHLIB_COMPAT (libpthread, GLIBC_2_1, GLIBC_2_2)
compat_symbol (libc, __libc_pwrite64, pwrite, GLIBC_2_2);
# endif
#endif

#if SHLIB_COMPAT (libc, GLIBC_2_1, GLIBC_2_2)
compat_symbol (libc, __libc_pwrite64, pwrite64, GLIBC_2_2);
compat_symbol (libc, __libc_pwrite64, __pwrite64, GLIBC_2_2);
#endif
