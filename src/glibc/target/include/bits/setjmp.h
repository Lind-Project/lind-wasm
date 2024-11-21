/* Copyright (C) 2001-2024 Free Software Foundation, Inc.
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

/* Define the machine-dependent type `jmp_buf'.  x86-64 version.  */
#ifndef _BITS_SETJMP_H
#define _BITS_SETJMP_H  1

#if !defined _SETJMP_H && !defined _PTHREAD_H
# error "Never include <bits/setjmp.h> directly; use <setjmp.h> instead."
#endif

#include <bits/wordsize.h>

#ifndef _ASM

// theoritically, our jmp_buf in lind-wasm only needs to store an u64
// which is a hash of the unwind_data. Previously there is a macro that
// would declare the jmp_buf with different size based on __WORDSIZE,
// this is deleted since jmp_buf is dealt by wasmtime and wasmtime won't
// treat it differently whether or not __WORDSIZE is set.
typedef long int __jmp_buf[8];

#endif

#endif  /* bits/setjmp.h */
