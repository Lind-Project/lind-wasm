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
  locale_t oldloc = _NL_CURRENT_LOCALE;

  if (newloc != NULL)
    {
      const locale_t locobj
	= newloc == LC_GLOBAL_LOCALE ? &_nl_global_locale : newloc;
      __libc_tsd_set (locale_t, LOCALE, locobj);

      /* Lind-WASM: NL_CURRENT_INDIRECT per-category TLS pointer update
	 skipped.  wasm-ld cannot handle weak TLS references
	 (R_WASM_MEMORY_ADDR_TLS_SLEB against undefined/weak symbols).
	 The TSD update above is sufficient — _NL_CURRENT_LOCALE reads
	 from TSD and functions using it will see the correct locale.  */

      /* Update the special tsd cache of some locale data.  */
      __libc_tsd_set (const uint16_t *, CTYPE_B, (void *) locobj->__ctype_b);
      __libc_tsd_set (const int32_t *, CTYPE_TOLOWER,
		      (void *) locobj->__ctype_tolower);
      __libc_tsd_set (const int32_t *, CTYPE_TOUPPER,
		      (void *) locobj->__ctype_toupper);
    }

  return oldloc == &_nl_global_locale ? LC_GLOBAL_LOCALE : oldloc;
}
libc_hidden_def (__uselocale)
weak_alias (__uselocale, uselocale)
