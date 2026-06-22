/* Linux implementation of copy_file_range.
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
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <errno.h>
#include <unistd.h>
#include <stdint.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

ssize_t
copy_file_range (int infd, __off64_t *pinoff,
                 int outfd, __off64_t *poutoff,
                 size_t length, unsigned int flags)
{
  uint64_t host_pinoff = pinoff == NULL
                         ? 0
                         : TRANSLATE_GUEST_POINTER_TO_HOST (pinoff);
  uint64_t host_poutoff = poutoff == NULL
                          ? 0
                          : TRANSLATE_GUEST_POINTER_TO_HOST (poutoff);

  return MAKE_LEGACY_SYSCALL (COPY_FILE_RANGE_SYSCALL,
                              "syscall|copy_file_range",
                              (uint64_t) infd,
                              host_pinoff,
                              (uint64_t) outfd,
                              host_poutoff,
                              (uint64_t) length,
                              (uint64_t) flags,
                              TRANSLATE_ERRNO_ON);
}
