/* __longjmp_cancel for x86.
   Copyright (C) 2018-2024 Free Software Foundation, Inc.
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

/* On wasm there is no shadow stack to skip, so __longjmp_cancel is identical
   to __longjmp.  The caller (sysdeps/unix/sysv/linux/x86/longjmp.c) passes
   two arguments (jmp_buf, val), so the signature must match.  */
#include <setjmp.h>

extern void __longjmp (__jmp_buf env, int val) __attribute__ ((__noreturn__));

void __longjmp_cancel (__jmp_buf env, int val)
{
  __longjmp (env, val);
}
