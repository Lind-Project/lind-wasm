/* User interface for extracting locale-dependent parameters.
   Copyright (C) 1995-2024 Free Software Foundation, Inc.
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

#include <langinfo.h>
#include <locale.h>
#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
#include "localeinfo.h"


/* Return a string with the data for locale-dependent parameter ITEM.  */

char *
__nl_langinfo_l (nl_item item, locale_t l)
{
  // Bug: locale related stuff is not working currently
  // this feature is not important so we might look into this later
  // if we want to support it in the future - Qianxi Chen
  
  return (char *) "ANSI_X3.4-1968";
  // Lind-Wasm: Original glibc code removed for compatibility
  // to find original source code refer to (2.39.9000) at (locale/nl_langinfo_l.c):(LINE 32-66)
}
libc_hidden_def (__nl_langinfo_l)
weak_alias (__nl_langinfo_l, nl_langinfo_l)
