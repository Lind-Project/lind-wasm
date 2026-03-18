/* Get/set resource limits.  Linux specific syscall.
   Copyright (C) 2021-2024 Free Software Foundation, Inc.
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

#define prlimit __redirect_prlimit
#include <sys/resource.h>
#undef prlimit
#include <sysdep.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>
int
__prlimit64 (pid_t pid, enum __rlimit_resource resource,
	     const struct rlimit64 *new_rlimit, struct rlimit64 *old_rlimit)
{
   uint64_t pnew = new_rlimit ? //null for getrlimit
   TRANSLATE_GUEST_POINTER_TO_HOST(new_rlimit) : 0; 
   uint64_t pold = old_rlimit ? //null for setrlimit
   TRANSLATE_GUEST_POINTER_TO_HOST(old_rlimit) : 0;

  return MAKE_LEGACY_SYSCALL(PRLIMIT64_SYSCALL, "syscall|prlimit64",
        (uint64_t) pid, (uint64_t) resource,
        pnew, pold,
        NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
#ifdef VERSION_prlimit64
# include <shlib-compat.h>
versioned_symbol (libc, __prlimit64, prlimit64, VERSION_prlimit64);
#else
strong_alias (__prlimit64, prlimit64)
# if __RLIM_T_MATCHES_RLIM64_T
strong_alias (prlimit64, prlimit)
# endif
#endif
