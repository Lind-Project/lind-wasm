/* Check IFUNC resolver with CPU_FEATURE_USABLE.
   Copyright (C) 2021-2024 Free Software Foundation, Inc.
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

#include <stdlib.h>
#include <support/test-driver.h>
#include "tst-ifunc-isa.h"

static int
do_test (void)
{
#if defined __clang__ && defined __i386__
  return EXIT_UNSUPPORTED;
#else
  enum isa value = foo ();
  enum isa expected = get_isa ();
  return value == expected ? EXIT_SUCCESS : EXIT_FAILURE;
#endif
}

#include <support/test-driver.c>
