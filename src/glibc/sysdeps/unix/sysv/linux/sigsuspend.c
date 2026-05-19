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
   wait until a signal arrives, and restore the set of blocked signals.  */

#ifdef LIND_EH_SETJMP
/* In EH mode, signals that call siglongjmp cannot be delivered via the
   Rust epoch path (signal_func.call() creates a Rust boundary that traps
   the __c_longjmp exception).  Instead, we set the mask WITHOUT triggering
   the epoch, then call __libc_pause() which delivers via lind-take-next-signal
   + signal_callback — a pure wasm call where siglongjmp propagates freely. */

/* Rust host import: atomically set cage.sigset to new_mask without triggering
   the epoch interrupt.  Returns the old mask. */
static int __imported_wasi_lind_sigsuspend_setmask (int new_mask)
  __attribute__ ((__import_module__ ("lind"),
                  __import_name__ ("lind-sigsuspend-setmask")));

extern int __libc_pause (void);

int
__sigsuspend (const sigset_t *set)
{
  int new_mask = *(const int *)set;
  int old_mask = __imported_wasi_lind_sigsuspend_setmask (new_mask);

  /* Deliver pending signals as pure wasm calls (no Rust boundary),
     allowing siglongjmp to propagate through the call stack.  */
  __libc_pause ();

  /* If we reach here, the handler returned normally.  Restore original mask.
     (sigprocmask may trigger epoch, but no siglongjmp concern at this point.) */
  sigset_t old;
  *(int *)&old = old_mask;
  sigprocmask (SIG_SETMASK, &old, NULL);

  __set_errno (EINTR);
  return -1;
}
#else
/* Asyncify mode: epoch-based delivery.  sigprocmask triggers the epoch when
   unblocking pending signals; sched_yield lets it fire synchronously.  */
int
__sigsuspend (const sigset_t *set)
{
  sigset_t old;
  sigprocmask (SIG_SETMASK, set, &old);
  sched_yield ();
  sigprocmask (SIG_SETMASK, &old, NULL);
  __set_errno (EINTR);
  return -1;
}
#endif
libc_hidden_def (__sigsuspend)
weak_alias (__sigsuspend, sigsuspend)
strong_alias (__sigsuspend, __libc_sigsuspend)
