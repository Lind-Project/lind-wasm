/* Copyright (C) 1996-2024 Free Software Foundation, Inc.
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

#include <signal.h>
#include <errno.h>
#include <stddef.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Change the set of blocked signals to SET, wait until a signal arrives,
   and restore the set of blocked signals.  Always returns -1 / EINTR.

   Implemented as a single rt_sigsuspend syscall so that the mask swap and
   the spin-wait are atomic from the wasm perspective: no epoch injection
   point can fire between the mask change and the start of the wait.  The
   old mask is saved here in wasm before the syscall and restored after,
   so signal_callback fires (at the entry of sigprocmask) after the syscall
   returns but before the mask is restored — correct POSIX delivery order.  */
int
__sigsuspend (const sigset_t *set)
{
  unsigned long long rawposix_set;
  sigset_t old;

  if (set == NULL)
    {
      __set_errno (EINVAL);
      return -1;
    }

  rawposix_set = set->__val[0];

  /* Save current mask for restoration after the syscall (read-only call,
     no pending-signal check, no epoch trigger).  */
  __sigprocmask (SIG_BLOCK, NULL, &old);

  /* Atomically install the new mask and wait for a signal.  */
  MAKE_LEGACY_SYSCALL (RT_SIGSUSPEND_SYSCALL, "syscall|sigsuspend",
                       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&rawposix_set),
                       NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED,
                       TRANSLATE_ERRNO_ON);

  /* Restore old mask.  signal_callback fires at the function entry of
     sigprocmask, delivering the pending signal before the mask changes.  */
  __sigprocmask (SIG_SETMASK, &old, NULL);
  __set_errno (EINTR);
  return -1;
}
libc_hidden_def (__sigsuspend)
weak_alias (__sigsuspend, sigsuspend)
strong_alias (__sigsuspend, __libc_sigsuspend)
