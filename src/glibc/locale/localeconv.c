/* Copyright (C) 1991-2024 Free Software Foundation, Inc.
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

#include <locale.h>
#include "localeinfo.h"
#include <shlib-compat.h>

/* Return monetary and numeric information about the current locale.  */
struct lconv *
__localeconv (void)
{
  static struct lconv result;

  // BUG: locale related stuff is not working currently
  // this feature is not important so we might look into this later
  // if we want to support it in the future - Qianxi Chen

  // Lind-Wasm: Original glibc code removed for compatibility
  // to find original source code refer to (2.39.9000) at (locale/localeconv.c):(LINE 28-32)

  return &result;
}

versioned_symbol (libc, __localeconv, localeconv, GLIBC_2_2);
#if SHLIB_COMPAT (libc, GLIBC_2_0, GLIBC_2_2)
strong_alias (__localeconv, __localeconv20)
compat_symbol (libc, __localeconv20, localeconv, GLIBC_2_0);
#endif
