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
#include <stdio.h>

/* Get and/or change the set of blocked signals.  */
int
__sigprocmask (int how, const sigset_t *set, sigset_t *oset)
{
   unsigned long long rawposix_set, rawposix_oset;
   rawposix_set = set->__val[0];
   int retval = MAKE_SYSCALL(149, "syscall|sigprocmask", (uint64_t) how, (uint64_t) &rawposix_set, (uint64_t) &rawposix_oset, NOTUSED, NOTUSED, NOTUSED);
   oset->__val[0] = (unsigned long int) rawposix_oset;
   return retval;
}
libc_hidden_def (__sigprocmask)
weak_alias (__sigprocmask, sigprocmask)
