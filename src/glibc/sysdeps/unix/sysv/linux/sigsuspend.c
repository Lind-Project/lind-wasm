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
#include <sched.h>

/* Change the set of blocked signals to SET,
   wait until a signal arrives, and restore the set of blocked signals.

   In Lind-WASM, signal delivery is epoch-based: sigprocmask sets the
   epoch to 0xc0ffee when unblocking pending signals, and the next WASM
   function call or loop back-edge fires the epoch callback which
   delivers all pending unblocked signals synchronously.  So we
   implement sigsuspend by swapping the mask, yielding (to trigger
   epoch-based delivery), and restoring the original mask.

   TODO: This only handles the case where signals are already pending
   when sigsuspend is called (e.g., kill then sigsuspend).  A real
   sigsuspend should block until a signal arrives, which would require
   a condvar in the Cage struct that lind_send_signal notifies.  */
int
__sigsuspend (const sigset_t *set)
{
  sigset_t old;
  /* Atomically replace the signal mask.  sigprocmask triggers the
     epoch if any pending signals become unblocked.  */
  sigprocmask (SIG_SETMASK, set, &old);

  /* Yield so the epoch callback fires and delivers pending signals
     synchronously before sched_yield's host call runs.  */
  sched_yield ();

  /* Restore the original mask.  */
  sigprocmask (SIG_SETMASK, &old, NULL);

  __set_errno (EINTR);
  return -1;
}
libc_hidden_def (__sigsuspend)
weak_alias (__sigsuspend, sigsuspend)
strong_alias (__sigsuspend, __libc_sigsuspend)
