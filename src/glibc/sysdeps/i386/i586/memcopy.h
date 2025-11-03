/* memcopy.h -- definitions for memory copy functions.  Pentium version.
   Copyright (C) 1994-2024 Free Software Foundation, Inc.
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

/* Get the i386 definitions.  We will override some of them below.  */
#include <sysdeps/i386/memcopy.h>

/* Written like this, the Pentium pipeline can execute the loop at a
   sustained rate of 2 instructions/clock, or asymptotically 480
   Mbytes/second at 60Mhz.  */

#undef WORD_COPY_FWD
#define WORD_COPY_FWD(dst_bp, src_bp, nbytes_left, nbytes)                    \
  do                                                                          \
    {                                                                         \
      size_t __nbytes = (nbytes);                                             \
      char *dst = (char *) (dst_bp);                                          \
      const char *src = (const char *) (src_bp);                              \
      while (__nbytes >= 32)                                                  \
	{                                                                     \
	  *(long *) (dst + 0) = *(const long *) (src + 0);                    \
	  *(long *) (dst + 4) = *(const long *) (src + 4);                    \
	  *(long *) (dst + 8) = *(const long *) (src + 8);                    \
	  *(long *) (dst + 12) = *(const long *) (src + 12);                  \
	  *(long *) (dst + 16) = *(const long *) (src + 16);                  \
	  *(long *) (dst + 20) = *(const long *) (src + 20);                  \
	  *(long *) (dst + 24) = *(const long *) (src + 24);                  \
	  *(long *) (dst + 28) = *(const long *) (src + 28);                  \
	  src += 32;                                                          \
	  dst += 32;                                                          \
	  __nbytes -= 32;                                                     \
	}                                                                     \
      nbytes_left = __nbytes;                                                 \
    }                                                                         \
  while (0)

#undef WORD_COPY_BWD
#define WORD_COPY_BWD(dst_ep, src_ep, nbytes_left, nbytes)                    \
  do                                                                          \
    {                                                                         \
      size_t __nbytes = (nbytes);                                             \
      char *dst = (char *) (dst_ep);                                          \
      const char *src = (const char *) (src_ep);                              \
      dst -= 32;                                                              \
      src -= 32;                                                              \
      while (__nbytes >= 32)                                                  \
	{                                                                     \
	  *(long *) (dst + 24) = *(const long *) (src + 24);                  \
	  *(long *) (dst + 20) = *(const long *) (src + 20);                  \
	  *(long *) (dst + 16) = *(const long *) (src + 16);                  \
	  *(long *) (dst + 12) = *(const long *) (src + 12);                  \
	  *(long *) (dst + 8) = *(const long *) (src + 8);                    \
	  *(long *) (dst + 4) = *(const long *) (src + 4);                    \
	  *(long *) (dst + 0) = *(const long *) (src + 0);                    \
	  src -= 32;                                                          \
	  dst -= 32;                                                          \
	  __nbytes -= 32;                                                     \
	}                                                                     \
      nbytes_left = __nbytes;                                                 \
    }                                                                         \
  while (0)
