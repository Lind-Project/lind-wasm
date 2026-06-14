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

   The rt_sigsuspend syscall atomically saves the current mask into
   rawposix_old, installs the new mask, checks for pending signals, and
   spins — all inside a single host call with no epoch injection points
   between any of those steps.  The restore call after the syscall runs
   back in wasm, so signal_callback fires at its function-header epoch
   point and delivers the signal before the mask is restored.  */
int
__sigsuspend (const sigset_t *set)
{
  unsigned long long rawposix_set;
  unsigned long long rawposix_old = 0;
  sigset_t old;

  if (set == NULL)
    {
      __set_errno (EINVAL);
      return -1;
    }

  rawposix_set = set->__val[0];

  /* Atomically save old mask, install new mask, and wait for a signal.  */
  MAKE_LEGACY_SYSCALL (RT_SIGSUSPEND_SYSCALL, "syscall|sigsuspend",
                       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&rawposix_set),
                       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (&rawposix_old),
                       NOTUSED, NOTUSED, NOTUSED, NOTUSED,
                       TRANSLATE_ERRNO_ON);

  /* Restore old mask.  signal_callback fires at the function entry of
     sigprocmask, delivering the pending signal before the mask changes.  */
  old.__val[0] = (unsigned long int) rawposix_old;
  __sigprocmask (SIG_SETMASK, &old, NULL);
  __set_errno (EINTR);
  return -1;
}
libc_hidden_def (__sigsuspend)
weak_alias (__sigsuspend, sigsuspend)
strong_alias (__sigsuspend, __libc_sigsuspend)
