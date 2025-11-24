/* Copyright (C) 1997-2024 Free Software Foundation, Inc.
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
#include <pthreadP.h>              /* SIGCANCEL, SIGSETXID */
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Get and/or change the set of blocked signals.  */
int
__sigprocmask (int how, const sigset_t *set, sigset_t *oset)
{
   // we do the manual translation between glibc sigset type and rawposix sigset type here
   unsigned long long rawposix_set, rawposix_oset;
   // check for NULL pointer
   if (set)
      rawposix_set = set->__val[0];
   int retval = MAKE_LEGACY_SYSCALL (SIGPROCMASK_SYSCALL, "syscall|sigprocmask", (uint64_t) how,(uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(set ? &rawposix_set : NULL),(uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST(oset ? &rawposix_oset : NULL), NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
   // check for NULL pointer
   if (oset)
      oset->__val[0] = (unsigned long int) rawposix_oset;
   return retval;
}
libc_hidden_def (__sigprocmask)
weak_alias (__sigprocmask, sigprocmask)
