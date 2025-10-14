/* mmap - map files or devices into memory.  Linux version.
   Copyright (C) 1999-2024 Free Software Foundation, Inc.
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
#include <unistd.h>
#include <sys/mman.h>
#include <sysdep.h>
#include <mmap_internal.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

void *
__mmap64 (void *addr, size_t len, int prot, int flags, int fd, off64_t offset)
{
  return MAKE_SYSCALL(MMAP_SYSCALL, "syscall|mmap", (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(addr), (uint64_t) len, (uint64_t) prot, (uint64_t) flags, (uint64_t) fd, (uint64_t) offset);
}
weak_alias (__mmap64, mmap64)
libc_hidden_def (__mmap64)

#ifdef __OFF_T_MATCHES_OFF64_T
weak_alias (__mmap64, mmap)
weak_alias (__mmap64, __mmap)
libc_hidden_def (__mmap)
#endif
