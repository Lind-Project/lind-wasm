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


// entry point of epoch callback in glibc, invoked by wasmtime
__attribute__((export_name("signal_callback")))
void signal_callback(__sighandler_t callback, int signal) {
// directly call into user's custom signal handler
if(callback != 0)
  callback(signal);
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
int retval = MAKE_SYSCALL(SIGACTION_SYSCALL, "syscall|sigaction", (uint64_t) sig, (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(act ? &rawposix_act : NULL), (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(oact ? &rawposix_oact : NULL), NOTUSED, NOTUSED, NOTUSED);

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
