/* POSIX.1 `sigaction' call for Linux/i386.
   Copyright (C) 1991-2024 Free Software Foundation, Inc.
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
#include <ldsodefs.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

#define SA_RESTORER 0x04000000

extern void restore_rt (void) {

}
extern void restore (void) {
  
}

// #define SET_SA_RESTORER(kact, act)				\
//   ({								\
//      if (GLRO(dl_sysinfo_dso) == NULL)				\
//        {							\
// 	 (kact)->sa_flags |= SA_RESTORER;			\
//          (kact)->sa_restorer = (((act)->sa_flags & SA_SIGINFO)	\
// 			       ? &restore_rt : &restore);	\
//        }							\
//      else							\
//        (kact)->sa_restorer = NULL;				\
//   })

// #define RESET_SA_RESTORER(act, kact) \
//   (act)->sa_restorer = (kact)->sa_restorer

// #include <sysdeps/unix/sysv/linux/libc_sigaction.c>

// /* NOTE: Please think twice before making any changes to the bits of
//    code below.  GDB needs some intimate knowledge about it to
//    recognize them as signal trampolines, and make backtraces through
//    signal handlers work right.  Important are both the names
//    (__restore and __restore_rt) and the exact instruction sequence.
//    If you ever feel the need to make any changes, please notify the
//    appropriate GDB maintainer.  */

// #define RESTORE(name, syscall) RESTORE2 (name, syscall)
// #define RESTORE2(name, syscall) \
// asm						\
//   (						\
//    ".text\n"					\
//    "	.align 16\n"				\
//    "__" #name ":\n"				\
//    "	movl $" #syscall ", %eax\n"		\
//    "	int  $0x80"				\
//    );

// /* The return code for realtime-signals.  */
// RESTORE (restore_rt, __NR_rt_sigreturn)

// /* For the boring old signals.  */
// #undef RESTORE2
// #define RESTORE2(name, syscall) \
// asm						\
//   (						\
//    ".text\n"					\
//    "	.align 8\n"				\
//    "__" #name ":\n"				\
//    "	popl %eax\n"				\
//    "	movl $" #syscall ", %eax\n"		\
//    "	int  $0x80"				\
//    );

// RESTORE (restore, __NR_sigreturn)

// entry point of epoch callback in glibc, invoked by wasmtime
// for signal handlers with signature void(int) — the POSIX-standard case
__attribute__((export_name("signal_callback")))
void signal_callback(__sighandler_t callback, int signal) {
  // directly call into user's custom signal handler
  if(callback != 0)
    callback(signal);
}

// alternate entry point for signal handlers declared with no parameters
// e.g. void handler() {} — valid C but compiles to ()->() in WASM
// call_indirect strictly checks types, so we need a separate trampoline
__attribute__((export_name("signal_callback_noarg")))
void signal_callback_noarg(int callback_idx, int signal) {
  if(callback_idx != 0)
    ((void (*)(void))callback_idx)();
}

// rawposix sigaction struct
struct rawposix_sigaction {
  __sighandler_t handler;
  unsigned long long sa_mask;
  int sa_flags;
};

int
__libc_sigaction (int sig, const struct sigaction *act, struct sigaction *oact)
{
  // we do the manual translation between glibc sigaction struct and rawposix sigaction struct here
  struct rawposix_sigaction rawposix_act, rawposix_oact;
  // check for NULL pointer
  if (act)
  {
    rawposix_act.handler = act->sa_handler;
    rawposix_act.sa_mask = act->sa_mask.__val[0];
    rawposix_act.sa_flags = act->sa_flags;
  }
  int retval = MAKE_LEGACY_SYSCALL(SIGACTION_SYSCALL, "syscall|sigaction", (uint64_t) sig, (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(act ? &rawposix_act : NULL), (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(oact ? &rawposix_oact : NULL), NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);

  // check for NULL pointer
  if (oact)
  {
    oact->sa_handler = rawposix_oact.handler;
    oact->sa_mask.__val[0] = rawposix_oact.sa_mask;
    oact->sa_flags = rawposix_oact.sa_flags;
  }

  return retval;
}
libc_hidden_def (__libc_sigaction)
