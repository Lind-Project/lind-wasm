/* Linux setrlimit implementation (32 bits off_t).
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
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#include <sys/resource.h>
#include <sysdep.h>
#include <shlib-compat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#if !__RLIM_T_MATCHES_RLIM64_T

/* The compatibility symbol is meant to match the old __NR_getrlimit syscall
   (with broken RLIM_INFINITY definition).  It should be provided iff
   __NR_getrlimit and __NR_ugetrlimit are both defined.  */
# ifndef __NR_ugetrlimit
#  undef SHLIB_COMPAT
#  define SHLIB_COMPAT(a, b, c) 0
# endif

int
__setrlimit (enum __rlimit_resource resource, const struct rlimit *rlim)
{
  /* Route through the lind syscall path, exactly as getrlimit.c does.

     The generic implementation used INLINE_SYSCALL_CALL(prlimit64, ...),
     which is not wired up in this port: it never reached rawposix and simply
     returned 0.  A guest that lowered a limit was therefore told it had
     succeeded while nothing changed -- a silent success, which is worse than
     a refusal because the caller has no way to detect it.

     Pass the 32-bit `struct rlimit` straight through rather than converting
     to `struct rlimit64` first.  rawposix reads this argument as sysdefs'
     `Rlimit { rlim_cur: u32, rlim_max: u32 }`, which is precisely the layout
     of `struct rlimit` on wasm32; handing it a `struct rlimit64` would make
     it read the high half of rlim_cur as rlim_max.  */
  uint64_t pnew = rlim ? TRANSLATE_GUEST_POINTER_TO_HOST(rlim) : 0;
  return MAKE_LEGACY_SYSCALL(PRLIMIT64_SYSCALL, "syscall|prlimit64",
      0, (uint64_t) resource,
      pnew, 0,
      NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

libc_hidden_def (__setrlimit)
# if SHLIB_COMPAT (libc, GLIBC_2_0, GLIBC_2_2)
strong_alias (__setrlimit, __setrlimit_1)
compat_symbol (libc, __setrlimit, setrlimit, GLIBC_2_0);
versioned_symbol (libc, __setrlimit_1, setrlimit, GLIBC_2_2);
# else
weak_alias (__setrlimit, setrlimit)
# endif

#endif /* __RLIM_T_MATCHES_RLIM64_T  */
