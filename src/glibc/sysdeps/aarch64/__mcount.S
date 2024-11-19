/* AArch64 definitions for profiling support.
   Copyright (C) 2024 Free Software Foundation, Inc.
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
   License along with the GNU C Library.  If not, see
   <https://www.gnu.org/licenses/>.  */

#include <sysdep.h>
#include <shlib-compat.h>

ENTRY (__mcount)
	stp     x29, x30, [sp, -16]!
	/* Note: xpaclri is needed for frompc because of gcc PR target/94791.  */
	hint	7
	mov	x1, x30
	/* Load the caller return address as frompc.  */
	ldr	x0, [x29, #8]
	mov	x30, x0
	mov	x0, x30
	/* Use the return address as selfpc.  */
	mov	x0, x30
	ldp     x29, x30, [sp], 16
	b	__mcount_internal
END (__mcount)

/* We forgot to add _mcount in glibc 2.17.  We added it in 2.18
   therefore we want it to be added with version GLIBC_2_18.  However,
   setting the version is not straight forward because a generic
   Version file includes an earlier 2.xx version for each this symbol
   and the linker uses the first version it sees.  */

#if SHLIB_COMPAT (libc, GLIBC_2_17, GLIBC_2_18)
versioned_symbol (libc, __mcount, _mcount, GLIBC_2_18);
#else
strong_alias (__mcount, _mcount);
#endif
