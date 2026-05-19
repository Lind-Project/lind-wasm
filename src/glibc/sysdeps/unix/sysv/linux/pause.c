/* Linux pause syscall implementation.
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
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#include <signal.h>
#include <unistd.h>
#include <sysdep-cancel.h>

#ifdef LIND_EH_SETJMP
/* In EH mode, signals cannot be delivered via signal_func.call() (a Rust→wasm
   call) because __c_longjmp thrown by siglongjmp inside the handler cannot
   cross that boundary back to the sigsetjmp catch block.  Instead, pause()
   polls for pending custom signals and calls signal_callback directly — a pure
   wasm call with no Rust boundary — so __c_longjmp can propagate freely. */

/* Declared with export_name("signal_callback") in libc_sigaction.c. */
extern void signal_callback (__sighandler_t callback, int signal);

/* Rust host import: returns a packed i64: high 32 bits = handler, low 32 bits
   = signo if a custom signal is pending; returns -1 when the queue is empty.
   SIG_DFL/SIG_IGN are handled on the Rust side before returning. */
static long long __imported_wasi_lind_take_next_signal (void)
  __attribute__ ((__import_module__ ("lind"),
                  __import_name__ ("lind-take-next-signal")));

/* Rust host import: restore the cage signal mask to the pre-delivery value
   saved when the signal was taken.  Called after a handler returns normally
   (i.e. without siglongjmp). */
static void __imported_wasi_lind_restore_signal_mask (void)
  __attribute__ ((__import_module__ ("lind"),
                  __import_name__ ("lind-restore-signal-mask")));

int
__libc_pause (void)
{
  long long packed;
  while ((packed = __imported_wasi_lind_take_next_signal ()) >= 0)
    {
      int handler = (int) (packed >> 32);
      int signo   = (int) (packed & 0xffffffff);
      /* Direct wasm call — no Rust boundary.  If the handler calls
         siglongjmp, __c_longjmp propagates through here and up to the
         LLVM-generated try/catch at the sigsetjmp call site. */
      signal_callback ((__sighandler_t) (uintptr_t) handler, signo);
      /* Handler returned normally (didn't call siglongjmp).
         Restore the signal mask that was saved before delivery. */
      __imported_wasi_lind_restore_signal_mask ();
    }
  __set_errno (EINTR);
  return -1;
}
#else
/* Suspend the process until a signal arrives.
   This always returns -1 and sets errno to EINTR.  */
int
__libc_pause (void)
{
# ifdef __NR_pause
  return SYSCALL_CANCEL (pause);
# else
  return SYSCALL_CANCEL (ppoll, NULL, 0, NULL, NULL);
# endif
}
#endif
weak_alias (__libc_pause, pause)
