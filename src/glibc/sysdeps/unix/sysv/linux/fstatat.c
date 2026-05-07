/* Get file status.  Linux version.
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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

#include <sys/stat.h>
#include <kernel_stat.h>
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#if !XSTAT_IS_XSTAT64
int
__fstatat (int fd, const char *file, struct stat *buf, int flag)
{
  /* lind-wasm: call rawposix directly to avoid the __stat64_t64 intermediate
     buffer whose layout mismatches StatData, which would cause spurious
     EOVERFLOW from the range checks below. */
  uint64_t host_file = TRANSLATE_GUEST_POINTER_TO_HOST (file);
  uint64_t host_buf = TRANSLATE_GUEST_POINTER_TO_HOST (buf);
  return MAKE_LEGACY_SYSCALL (FSTATAT_SYSCALL, "syscall|fstatat",
      (uint64_t) fd, host_file, host_buf, (uint64_t) flag,
      NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

weak_alias (__fstatat, fstatat)
#endif
