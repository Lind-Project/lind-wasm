/* @(#)s_lib_ver.c 5.1 93/09/24 */
/*
 * ====================================================
 * Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
 *
 * Developed at SunPro, a Sun Microsystems, Inc. business.
 * Permission to use, copy, modify, and distribute this
 * software is freely granted, provided that this notice
 * is preserved.
 * ====================================================
 */

#include <math-svid-compat.h>

/* Define and initialize _LIB_VERSION.
 *
 * The generic sysdeps/ieee754/s_lib_version.c uses compat_symbol() to alias
 * _LIB_VERSION_INTERNAL -> _LIB_VERSION at the GLIBC_2.0 symbol version.
 * In C mode, compat_symbol() expands to _set_symbol_version() which is a
 * no-op (the .symver assembler directive is only emitted in __ASSEMBLER__
 * mode).  WASM has no ELF symbol versioning at all, so _LIB_VERSION is
 * never defined and ends up as an imported GOT.mem global.
 *
 * Override with a plain weak_alias so the symbol is defined within libm
 * itself and does not need to be imported.
 */
#undef _LIB_VERSION
#if LIBM_SVID_COMPAT
_LIB_VERSION_TYPE _LIB_VERSION_INTERNAL = _POSIX_;
weak_alias (_LIB_VERSION_INTERNAL, _LIB_VERSION);
#endif
