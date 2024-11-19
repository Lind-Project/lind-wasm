/* Copyright (C) 1991-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   The GNU C Library is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 2.1 of the License, or (at your option) any later version.

   In addition to the permissions in the GNU Lesser General Public
   License, the Free Software Foundation gives you unlimited
   permission to link the compiled version of this file with other
   programs, and to distribute those programs without any restriction
   coming from the use of this file. (The GNU Lesser General Public
   License restrictions do apply in other respects; for example, they
   cover modification of the file, and distribution when not linked
   into another program.)

   Note that people who make modified versions of this file are not
   obligated to grant this special exception for their modified
   versions; it is their choice whether to do so. The GNU Lesser
   General Public License gives permission to release a modified
   version without this exception; this exception also makes it
   possible to release a modified version which carries forward this
   exception.

   The GNU C Library is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <stdlib.h>
#include <dso_handle.h>
#include <assert.h>
#include <stdint.h>

#include <libc-lock.h>
#include "exit.h"
#include <pointer_guard.h>

int
attribute_hidden
__internal_atexit_2 (void (*func) (void),
		   struct exit_function_list **listp)
{
  struct exit_function *new;

  /* As a QoI issue we detect NULL early with an assertion instead
     of a SIGSEGV at program exit when the handler is run (bug 20544).  */
  assert (func != NULL);

  __libc_lock_lock (__exit_funcs_lock);
  new = __new_exitfn (listp);

  if (new == NULL)
    {
      __libc_lock_unlock (__exit_funcs_lock);
      return -1;
    }

  PTR_MANGLE (func);
  new->func.at = func;
  new->flavor = ef_at;
  __libc_lock_unlock (__exit_funcs_lock);
  return 0;
}

/* Register FUNC to be executed by `exit'.  */
int
#ifndef atexit
attribute_hidden
#endif
atexit (void (*func) (void))
{
  // glibc's implementation of atexit is a calling down to __cxa_atexit
  // which is a extension of normal atexit. This involves change the signature of the exit function.
  // This could work in c if you know exactly what you are doing. However, wasm apparently does not
  // allow to change function signature as it will cause function type mismatch error when invoke the function
  // So modifying its original __internal_atexit to allow it to handle normal atexit function - Qianxi Chen
  return __internal_atexit_2 (func, &__exit_funcs);
}
