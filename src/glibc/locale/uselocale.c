/* uselocale -- fetch and set the current per-thread locale
   Copyright (C) 2002-2024 Free Software Foundation, Inc.
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
#include <ctype.h>

/* Switch the current thread's locale to DATASET.
   If DATASET is null, instead just return the current setting.
   The special value LC_GLOBAL_LOCALE is the initial setting
   for all threads, and means the thread uses the global
   setting controlled by `setlocale'.  */
locale_t
__uselocale (locale_t newloc)
{
   // BUG: locale related stuff is not working currently
   // this feature is not important so we might look into this later
   // if we want to support it in the future - Qianxi Chen

   // Lind-Wasm: Original glibc code removed for compatibility
   // to find original source code refer to (2.39.9000) at (locale/uselocale.c):(LINE 31-72)
   return _NL_CURRENT_LOCALE;
}
libc_hidden_def (__uselocale)
weak_alias (__uselocale, uselocale)
