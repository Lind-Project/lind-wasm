/* Copyright (C) 2003-2024 Free Software Foundation, Inc.
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

#include <time.h>
#include <kernel-features.h>
#include <errno.h>

#include <sysdep-cancel.h>
#include "kernel-posix-cpu-timers.h"

#include <shlib-compat.h>

#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* We can simply use the syscall.  The CPU clocks are not supported
   with this function.  */
int
__clock_nanosleep_time64 (clockid_t clock_id, int flags,
			  const struct __timespec64 *req,
			  struct __timespec64 *rem)
{
  uint64_t host_req = TRANSLATE_GUEST_POINTER_TO_HOST (req);
  uint64_t host_rem = TRANSLATE_GUEST_POINTER_TO_HOST (rem);
  return MAKE_TRADITION (
      NANOSLEEP_TIME64_SYSCALL, "syscall|nanosleep", (uint64_t) clock_id,
      (uint64_t) flags, host_req, host_rem, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
