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

#include <array_length.h>
#include <assert.h>
#include <ctype.h>
#include <limits.h>
#include <printf.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <wchar.h>
#include <libc-lock.h>
#include <sys/param.h>
#include <_itoa.h>
#include <locale/localeinfo.h>
#include <grouping_iterator.h>
#include <stdio.h>
#include <scratch_buffer.h>
#include <intprops.h>
#include <printf_buffer.h>
#include <printf_buffer_to_file.h>

/* This code is shared between the standard stdio implementation found
   in GNU C library and the libio implementation originally found in
   GNU libg++.

   Beside this it is also shared between the normal and wide character
   implementation as defined in ISO/IEC 9899:1990/Amendment 1:1995.  */

#include <libioP.h>

#ifdef COMPILE_WPRINTF
#include <wctype.h>
#endif

#define ARGCHECK(S, Format) \
  do									     \
    {									     \
      /* Check file argument for consistence.  */			     \
      CHECK_FILE (S, -1);						     \
      if (S->_flags & _IO_NO_WRITES)					     \
       {								     \
	 S->_flags |= _IO_ERR_SEEN;					     \
	 __set_errno (EBADF);						     \
	 return -1;							     \
       }								     \
      if (Format == NULL)						     \
       {								     \
	 __set_errno (EINVAL);						     \
	 return -1;							     \
       }								     \
    } while (0)
#define UNBUFFERED_P(S) ((S)->_flags & _IO_UNBUFFERED)

#if __HAVE_FLOAT128_UNLIKE_LDBL
# define PARSE_FLOAT_VA_ARG_EXTENDED(INFO)				      \
  do									      \
    {									      \
      if (is_long_double						      \
	  && (mode_flags & PRINTF_LDBL_USES_FLOAT128) != 0)		      \
	{								      \
	  INFO.is_binary128 = 1;					      \
	  the_arg.pa_float128 = va_arg (ap, _Float128);			      \
	}								      \
      else								      \
	{								      \
	  PARSE_FLOAT_VA_ARG (INFO);					      \
	}								      \
    }									      \
  while (0)
#else
# define PARSE_FLOAT_VA_ARG_EXTENDED(INFO)				      \
  PARSE_FLOAT_VA_ARG (INFO);
#endif

#define PARSE_FLOAT_VA_ARG(INFO)					      \
  do									      \
    {									      \
      INFO.is_binary128 = 0;						      \
      if (is_long_double)						      \
	the_arg.pa_long_double = va_arg (ap, long double);		      \
      else								      \
	the_arg.pa_double = va_arg (ap, double);			      \
    }									      \
  while (0)

#if __HAVE_FLOAT128_UNLIKE_LDBL
# define SETUP_FLOAT128_INFO(INFO)					      \
  do									      \
    {									      \
      if ((mode_flags & PRINTF_LDBL_USES_FLOAT128) != 0)		      \
	INFO.is_binary128 = is_long_double;				      \
      else								      \
	INFO.is_binary128 = 0;						      \
    }									      \
  while (0)
#else
# define SETUP_FLOAT128_INFO(INFO)					      \
  do									      \
    {									      \
      INFO.is_binary128 = 0;						      \
    }									      \
  while (0)
#endif

#ifndef COMPILE_WPRINTF
# include "printf_buffer-char.h"
# define vfprintf	__vfprintf_internal
# define OTHER_CHAR_T   wchar_t
# define UCHAR_T	unsigned char
# define INT_T		int
typedef const char *THOUSANDS_SEP_T;
# define L_(Str)	Str
# define ISDIGIT(Ch)	((unsigned int) ((Ch) - '0') < 10)
# define STR_LEN(Str)	strlen (Str)

# define ORIENT		if (_IO_vtable_offset (s) == 0 && _IO_fwide (s, -1) != -1)\
			  return -1
# define CONVERT_FROM_OTHER_STRING __wcsrtombs
#else
# include "printf_buffer-wchar_t.h"
# define vfprintf	__vfwprintf_internal
# define OTHER_CHAR_T   char
/* This is a hack!!!  There should be a type uwchar_t.  */
# define UCHAR_T	unsigned int /* uwchar_t */
# define INT_T		wint_t
typedef wchar_t THOUSANDS_SEP_T;
# define L_(Str)	L##Str
# define ISDIGIT(Ch)	((unsigned int) ((Ch) - L'0') < 10)
# define STR_LEN(Str)	__wcslen (Str)

# include <_itowa.h>

# define ORIENT		if (_IO_fwide (s, 1) != 1) return -1
# define CONVERT_FROM_OTHER_STRING __mbsrtowcs

# undef _itoa
# define _itoa(Val, Buf, Base, Case) _itowa (Val, Buf, Base, Case)
# define _itoa_word(Val, Buf, Base, Case) _itowa_word (Val, Buf, Base, Case)
# undef EOF
# define EOF WEOF
#endif

/* Include the shared code for parsing the format string.  */
#include "printf-parse.h"


/* Write the string SRC to S.  If PREC is non-negative, write at most
   PREC bytes.  If LEFT is true, perform left justification.  */
static void
outstring_converted_wide_string (struct Xprintf_buffer *target,
				 const OTHER_CHAR_T *src, int prec,
				 int width, bool left)
{
  /* Use a small buffer to combine processing of multiple characters.
     CONVERT_FROM_OTHER_STRING expects the buffer size in (wide)
     characters, and buf_length counts that.  */
  enum { buf_length = 256 / sizeof (CHAR_T) };
  CHAR_T buf[buf_length];
  _Static_assert (sizeof (buf) > MB_LEN_MAX,
		  "buffer is large enough for a single multi-byte character");

  /* Add the initial padding if needed.  */
  if (width > 0 && !left)
    {
      /* Make a first pass to find the output width, so that we can
	 add the required padding.  */
      mbstate_t mbstate = { 0 };
      const OTHER_CHAR_T *src_copy = src;
      size_t total_written;
      if (prec < 0)
	total_written = CONVERT_FROM_OTHER_STRING
	  (NULL, &src_copy, 0, &mbstate);
      else
	{
	  /* The source might not be null-terminated.  Enforce the
	     limit manually, based on the output length.  */
	  total_written = 0;
	  size_t limit = prec;
	  while (limit > 0 && src_copy != NULL)
	    {
	      size_t write_limit = buf_length;
	      if (write_limit > limit)
		write_limit = limit;
	      size_t written = CONVERT_FROM_OTHER_STRING
		(buf, &src_copy, write_limit, &mbstate);
	      if (written == (size_t) -1)
		{
		  Xprintf_buffer_mark_failed (target);
		  return;
		}
	      if (written == 0)
		break;
	      total_written += written;
	      limit -= written;
	    }
	}

      /* Output initial padding.  */
      Xprintf_buffer_pad (target, L_(' '), width - total_written);
      if (Xprintf_buffer_has_failed (target))
	return;
    }

  /* Convert the input string, piece by piece.  */
  size_t total_written = 0;
  {
    mbstate_t mbstate = { 0 };
    /* If prec is negative, remaining is not decremented, otherwise,
      it serves as the write limit.  */
    size_t remaining = -1;
    if (prec >= 0)
      remaining = prec;
    while (remaining > 0 && src != NULL)
      {
	size_t write_limit = buf_length;
	if (remaining < write_limit)
	  write_limit = remaining;
	size_t written = CONVERT_FROM_OTHER_STRING
	  (buf, &src, write_limit, &mbstate);
	if (written == (size_t) -1)
	  {
	    Xprintf_buffer_mark_failed (target);
	    return;
	  }
	if (written == 0)
	  break;
	Xprintf_buffer_write (target, buf, written);
	total_written += written;
	if (prec >= 0)
	  remaining -= written;
      }
  }

  /* Add final padding.  */
  if (width > 0 && left)
    Xprintf_buffer_pad (target, L_(' '), width - total_written);
}

/* Calls __printf_fp or __printf_fphex based on the value of the
   format specifier INFO->spec.  */
static inline void
__printf_fp_spec (struct Xprintf_buffer *target,
		  const struct printf_info *info, const void *const *args)
{
  if (info->spec == 'a' || info->spec == 'A')
    Xprintf (fphex_l_buffer) (target, _NL_CURRENT_LOCALE, info, args);
  else
    Xprintf (fp_l_buffer) (target, _NL_CURRENT_LOCALE, info, args);
}

/* For handling long_double and longlong we use the same flag.  If
   `long' and `long long' are effectively the same type define it to
   zero.  */
#if LONG_MAX == LONG_LONG_MAX
# define is_longlong 0
#else
# define is_longlong is_long_double
#endif

/* If `long' and `int' is effectively the same type we don't have to
   handle `long separately.  */
#if INT_MAX == LONG_MAX
# define is_long_num	0
#else
# define is_long_num	is_long
#endif


/* Global constants.  */
static const CHAR_T null[] = L_("(null)");

/* Size of the work_buffer variable (in characters, not bytes.  */
enum { WORK_BUFFER_SIZE = 1000 / sizeof (CHAR_T) };

/* This table maps a character into a number representing a class.  In
   each step there is a destination label for each class.  */
static const uint8_t jump_table[] =
  {
    /* ' ' */  1,            0,            0, /* '#' */  4,
	       0, /* '%' */ 14,            0, /* '\''*/  6,
	       0,            0, /* '*' */  7, /* '+' */  2,
	       0, /* '-' */  3, /* '.' */  9,            0,
    /* '0' */  5, /* '1' */  8, /* '2' */  8, /* '3' */  8,
    /* '4' */  8, /* '5' */  8, /* '6' */  8, /* '7' */  8,
    /* '8' */  8, /* '9' */  8,            0,            0,
	       0,            0,            0,            0,
	       0, /* 'A' */ 26, /* 'B' */ 30, /* 'C' */ 25,
	       0, /* 'E' */ 19, /* F */   19, /* 'G' */ 19,
	       0, /* 'I' */ 29,            0,            0,
    /* 'L' */ 12,            0,            0,            0,
	       0,            0,            0, /* 'S' */ 21,
	       0,            0,            0,            0,
    /* 'X' */ 18,            0, /* 'Z' */ 13,            0,
	       0,            0,            0,            0,
	       0, /* 'a' */ 26, /* 'b' */ 30, /* 'c' */ 20,
    /* 'd' */ 15, /* 'e' */ 19, /* 'f' */ 19, /* 'g' */ 19,
    /* 'h' */ 10, /* 'i' */ 15, /* 'j' */ 28,            0,
    /* 'l' */ 11, /* 'm' */ 24, /* 'n' */ 23, /* 'o' */ 17,
    /* 'p' */ 22, /* 'q' */ 12,            0, /* 's' */ 21,
    /* 't' */ 27, /* 'u' */ 16,            0, /* 'w' */ 31,
    /* 'x' */ 18,            0, /* 'z' */ 13
  };

#define NOT_IN_JUMP_RANGE(Ch) ((Ch) < L_(' ') || (Ch) > L_('z'))
#define CHAR_CLASS(Ch) (jump_table[(INT_T) (Ch) - L_(' ')])
#define LABEL(Name) do_##Name
#ifdef SHARED
  /* 'int' is enough and it saves some space on 64 bit systems.  */
# define JUMP_TABLE_TYPE const int
# define JUMP_TABLE_BASE_LABEL do_form_unknown
# define REF(Name) &&do_##Name - &&JUMP_TABLE_BASE_LABEL
# define JUMP(ChExpr, table)						      \
      do								      \
	{								      \
	  int offset;							      \
	  void *ptr;							      \
	  spec = (ChExpr);						      \
	  offset = NOT_IN_JUMP_RANGE (spec) ? REF (form_unknown)	      \
	    : table[CHAR_CLASS (spec)];					      \
	  ptr = &&JUMP_TABLE_BASE_LABEL + offset;			      \
	  goto *ptr;							      \
	}								      \
      while (0)
#else
# define JUMP_TABLE_TYPE const void *const
# define REF(Name) &&do_##Name
# define JUMP(ChExpr, table)						      \
      do								      \
	{								      \
	  const void *ptr;						      \
	  spec = (ChExpr);						      \
	  ptr = NOT_IN_JUMP_RANGE (spec) ? REF (form_unknown)		      \
	    : table[CHAR_CLASS (spec)];					      \
	  goto *ptr;							      \
	}								      \
      while (0)
#endif

#define STEP0_3_TABLE							      \
    /* Step 0: at the beginning.  */					      \
    static JUMP_TABLE_TYPE step0_jumps[32] =				      \
    {									      \
      REF (form_unknown),						      \
      REF (flag_space),		/* for ' ' */				      \
      REF (flag_plus),		/* for '+' */				      \
      REF (flag_minus),		/* for '-' */				      \
      REF (flag_hash),		/* for '<hash>' */			      \
      REF (flag_zero),		/* for '0' */				      \
      REF (flag_quote),		/* for '\'' */				      \
      REF (width_asterics),	/* for '*' */				      \
      REF (width),		/* for '1'...'9' */			      \
      REF (precision),		/* for '.' */				      \
      REF (mod_half),		/* for 'h' */				      \
      REF (mod_long),		/* for 'l' */				      \
      REF (mod_longlong),	/* for 'L', 'q' */			      \
      REF (mod_size_t),		/* for 'z', 'Z' */			      \
      REF (form_percent),	/* for '%' */				      \
      REF (form_integer),	/* for 'd', 'i' */			      \
      REF (form_unsigned),	/* for 'u' */				      \
      REF (form_octal),		/* for 'o' */				      \
      REF (form_hexa),		/* for 'X', 'x' */			      \
      REF (form_float),		/* for 'E', 'e', 'F', 'f', 'G', 'g' */	      \
      REF (form_character),	/* for 'c' */				      \
      REF (form_string),	/* for 's', 'S' */			      \
      REF (form_pointer),	/* for 'p' */				      \
      REF (form_number),	/* for 'n' */				      \
      REF (form_strerror),	/* for 'm' */				      \
      REF (form_wcharacter),	/* for 'C' */				      \
      REF (form_floathex),	/* for 'A', 'a' */			      \
      REF (mod_ptrdiff_t),      /* for 't' */				      \
      REF (mod_intmax_t),       /* for 'j' */				      \
      REF (flag_i18n),		/* for 'I' */				      \
      REF (form_binary),	/* for 'B', 'b' */			      \
      REF (mod_bitwidth),	/* for 'w' */				      \
    };									      \
    /* Step 1: after processing width.  */				      \
    static JUMP_TABLE_TYPE step1_jumps[32] =				      \
    {									      \
      REF (form_unknown),						      \
      REF (form_unknown),	/* for ' ' */				      \
      REF (form_unknown),	/* for '+' */				      \
      REF (form_unknown),	/* for '-' */				      \
      REF (form_unknown),	/* for '<hash>' */			      \
      REF (form_unknown),	/* for '0' */				      \
      REF (form_unknown),	/* for '\'' */				      \
      REF (form_unknown),	/* for '*' */				      \
      REF (form_unknown),	/* for '1'...'9' */			      \
      REF (precision),		/* for '.' */				      \
      REF (mod_half),		/* for 'h' */				      \
      REF (mod_long),		/* for 'l' */				      \
      REF (mod_longlong),	/* for 'L', 'q' */			      \
      REF (mod_size_t),		/* for 'z', 'Z' */			      \
      REF (form_percent),	/* for '%' */				      \
      REF (form_integer),	/* for 'd', 'i' */			      \
      REF (form_unsigned),	/* for 'u' */				      \
      REF (form_octal),		/* for 'o' */				      \
      REF (form_hexa),		/* for 'X', 'x' */			      \
      REF (form_float),		/* for 'E', 'e', 'F', 'f', 'G', 'g' */	      \
      REF (form_character),	/* for 'c' */				      \
      REF (form_string),	/* for 's', 'S' */			      \
      REF (form_pointer),	/* for 'p' */				      \
      REF (form_number),	/* for 'n' */				      \
      REF (form_strerror),	/* for 'm' */				      \
      REF (form_wcharacter),	/* for 'C' */				      \
      REF (form_floathex),	/* for 'A', 'a' */			      \
      REF (mod_ptrdiff_t),      /* for 't' */				      \
      REF (mod_intmax_t),       /* for 'j' */				      \
      REF (form_unknown),       /* for 'I' */				      \
      REF (form_binary),	/* for 'B', 'b' */			      \
      REF (mod_bitwidth),	/* for 'w' */				      \
    };									      \
    /* Step 2: after processing precision.  */				      \
    static JUMP_TABLE_TYPE step2_jumps[32] =				      \
    {									      \
      REF (form_unknown),						      \
      REF (form_unknown),	/* for ' ' */				      \
      REF (form_unknown),	/* for '+' */				      \
      REF (form_unknown),	/* for '-' */				      \
      REF (form_unknown),	/* for '<hash>' */			      \
      REF (form_unknown),	/* for '0' */				      \
      REF (form_unknown),	/* for '\'' */				      \
      REF (form_unknown),	/* for '*' */				      \
      REF (form_unknown),	/* for '1'...'9' */			      \
      REF (form_unknown),	/* for '.' */				      \
      REF (mod_half),		/* for 'h' */				      \
      REF (mod_long),		/* for 'l' */				      \
      REF (mod_longlong),	/* for 'L', 'q' */			      \
      REF (mod_size_t),		/* for 'z', 'Z' */			      \
      REF (form_percent),	/* for '%' */				      \
      REF (form_integer),	/* for 'd', 'i' */			      \
      REF (form_unsigned),	/* for 'u' */				      \
      REF (form_octal),		/* for 'o' */				      \
      REF (form_hexa),		/* for 'X', 'x' */			      \
      REF (form_float),		/* for 'E', 'e', 'F', 'f', 'G', 'g' */	      \
      REF (form_character),	/* for 'c' */				      \
      REF (form_string),	/* for 's', 'S' */			      \
      REF (form_pointer),	/* for 'p' */				      \
      REF (form_number),	/* for 'n' */				      \
      REF (form_strerror),	/* for 'm' */				      \
      REF (form_wcharacter),	/* for 'C' */				      \
      REF (form_floathex),	/* for 'A', 'a' */			      \
      REF (mod_ptrdiff_t),      /* for 't' */				      \
      REF (mod_intmax_t),       /* for 'j' */				      \
      REF (form_unknown),       /* for 'I' */				      \
      REF (form_binary),	/* for 'B', 'b' */			      \
      REF (mod_bitwidth),	/* for 'w' */				      \
    };									      \
    /* Step 3a: after processing first 'h' modifier.  */		      \
    static JUMP_TABLE_TYPE step3a_jumps[32] =				      \
    {									      \
      REF (form_unknown),						      \
      REF (form_unknown),	/* for ' ' */				      \
      REF (form_unknown),	/* for '+' */				      \
      REF (form_unknown),	/* for '-' */				      \
      REF (form_unknown),	/* for '<hash>' */			      \
      REF (form_unknown),	/* for '0' */				      \
      REF (form_unknown),	/* for '\'' */				      \
      REF (form_unknown),	/* for '*' */				      \
      REF (form_unknown),	/* for '1'...'9' */			      \
      REF (form_unknown),	/* for '.' */				      \
      REF (mod_halfhalf),	/* for 'h' */				      \
      REF (form_unknown),	/* for 'l' */				      \
      REF (form_unknown),	/* for 'L', 'q' */			      \
      REF (form_unknown),	/* for 'z', 'Z' */			      \
      REF (form_percent),	/* for '%' */				      \
      REF (form_integer),	/* for 'd', 'i' */			      \
      REF (form_unsigned),	/* for 'u' */				      \
      REF (form_octal),		/* for 'o' */				      \
      REF (form_hexa),		/* for 'X', 'x' */			      \
      REF (form_unknown),	/* for 'E', 'e', 'F', 'f', 'G', 'g' */	      \
      REF (form_unknown),	/* for 'c' */				      \
      REF (form_unknown),	/* for 's', 'S' */			      \
      REF (form_unknown),	/* for 'p' */				      \
      REF (form_number),	/* for 'n' */				      \
      REF (form_unknown),	/* for 'm' */				      \
      REF (form_unknown),	/* for 'C' */				      \
      REF (form_unknown),	/* for 'A', 'a' */			      \
      REF (form_unknown),       /* for 't' */				      \
      REF (form_unknown),       /* for 'j' */				      \
      REF (form_unknown),       /* for 'I' */				      \
      REF (form_binary),	/* for 'B', 'b' */			      \
      REF (form_unknown),	/* for 'w' */				      \
    };									      \
    /* Step 3b: after processing first 'l' modifier.  */		      \
    static JUMP_TABLE_TYPE step3b_jumps[32] =				      \
    {									      \
      REF (form_unknown),						      \
      REF (form_unknown),	/* for ' ' */				      \
      REF (form_unknown),	/* for '+' */				      \
      REF (form_unknown),	/* for '-' */				      \
      REF (form_unknown),	/* for '<hash>' */			      \
      REF (form_unknown),	/* for '0' */				      \
      REF (form_unknown),	/* for '\'' */				      \
      REF (form_unknown),	/* for '*' */				      \
      REF (form_unknown),	/* for '1'...'9' */			      \
      REF (form_unknown),	/* for '.' */				      \
      REF (form_unknown),	/* for 'h' */				      \
      REF (mod_longlong),	/* for 'l' */				      \
      REF (form_unknown),	/* for 'L', 'q' */			      \
      REF (form_unknown),	/* for 'z', 'Z' */			      \
      REF (form_percent),	/* for '%' */				      \
      REF (form_integer),	/* for 'd', 'i' */			      \
      REF (form_unsigned),	/* for 'u' */				      \
      REF (form_octal),		/* for 'o' */				      \
      REF (form_hexa),		/* for 'X', 'x' */			      \
      REF (form_float),		/* for 'E', 'e', 'F', 'f', 'G', 'g' */	      \
      REF (form_character),	/* for 'c' */				      \
      REF (form_string),	/* for 's', 'S' */			      \
      REF (form_pointer),	/* for 'p' */				      \
      REF (form_number),	/* for 'n' */				      \
      REF (form_strerror),	/* for 'm' */				      \
      REF (form_wcharacter),	/* for 'C' */				      \
      REF (form_floathex),	/* for 'A', 'a' */			      \
      REF (form_unknown),       /* for 't' */				      \
      REF (form_unknown),       /* for 'j' */				      \
      REF (form_unknown),       /* for 'I' */				      \
      REF (form_binary),	/* for 'B', 'b' */			      \
      REF (form_unknown),	/* for 'w' */				      \
    }

#define STEP4_TABLE							      \
    /* Step 4: processing format specifier.  */				      \
    static JUMP_TABLE_TYPE step4_jumps[32] =				      \
    {									      \
      REF (form_unknown),						      \
      REF (form_unknown),	/* for ' ' */				      \
      REF (form_unknown),	/* for '+' */				      \
      REF (form_unknown),	/* for '-' */				      \
      REF (form_unknown),	/* for '<hash>' */			      \
      REF (form_unknown),	/* for '0' */				      \
      REF (form_unknown),	/* for '\'' */				      \
      REF (form_unknown),	/* for '*' */				      \
      REF (form_unknown),	/* for '1'...'9' */			      \
      REF (form_unknown),	/* for '.' */				      \
      REF (form_unknown),	/* for 'h' */				      \
      REF (form_unknown),	/* for 'l' */				      \
      REF (form_unknown),	/* for 'L', 'q' */			      \
      REF (form_unknown),	/* for 'z', 'Z' */			      \
      REF (form_percent),	/* for '%' */				      \
      REF (form_integer),	/* for 'd', 'i' */			      \
      REF (form_unsigned),	/* for 'u' */				      \
      REF (form_octal),		/* for 'o' */				      \
      REF (form_hexa),		/* for 'X', 'x' */			      \
      REF (form_float),		/* for 'E', 'e', 'F', 'f', 'G', 'g' */	      \
      REF (form_character),	/* for 'c' */				      \
      REF (form_string),	/* for 's', 'S' */			      \
      REF (form_pointer),	/* for 'p' */				      \
      REF (form_number),	/* for 'n' */				      \
      REF (form_strerror),	/* for 'm' */				      \
      REF (form_wcharacter),	/* for 'C' */				      \
      REF (form_floathex),	/* for 'A', 'a' */			      \
      REF (form_unknown),       /* for 't' */				      \
      REF (form_unknown),       /* for 'j' */				      \
      REF (form_unknown),       /* for 'I' */				      \
      REF (form_binary),	/* for 'B', 'b' */			      \
      REF (form_unknown),	/* for 'w' */				      \
    }

/* Handle positional format specifiers.  */
static void printf_positional (struct Xprintf_buffer *buf,
			       const CHAR_T *format, int readonly_format,
			       va_list ap, va_list *ap_savep,
			       int nspecs_done, const UCHAR_T *lead_str_end,
			       CHAR_T *work_buffer, int save_errno,
			       const char *grouping,
			       THOUSANDS_SEP_T thousands_sep,
			       unsigned int mode_flags);

/* Handle unknown format specifier.  */
static void printf_unknown (struct Xprintf_buffer *,
			    const struct printf_info *) __THROW;

static void group_number (struct Xprintf_buffer *buf,
			  struct grouping_iterator *iter,
			  CHAR_T *from, CHAR_T *to,
			  THOUSANDS_SEP_T thousands_sep, bool i18n);

/* The buffer-based function itself.  */
void
Xprintf_buffer (struct Xprintf_buffer *buf, const CHAR_T *format,
		  va_list ap, unsigned int mode_flags)
{
  /* The character used as thousands separator.  */
  THOUSANDS_SEP_T thousands_sep = 0;

  /* The string describing the size of groups of digits.  */
  const char *grouping;

  /* Current character in format string.  */
  const UCHAR_T *f;

  /* End of leading constant string.  */
  const UCHAR_T *lead_str_end;

  /* Points to next format specifier.  */
  const UCHAR_T *end_of_spec;

  /* Buffer intermediate results.  */
  CHAR_T work_buffer[WORK_BUFFER_SIZE];
  CHAR_T *workend;

  /* We have to save the original argument pointer.  */
  va_list ap_save;

  /* Count number of specifiers we already processed.  */
  int nspecs_done;

  /* For the %m format we may need the current `errno' value.  */
  int save_errno = errno;

  /* 1 if format is in read-only memory, -1 if it is in writable memory,
     0 if unknown.  */
  int readonly_format = 0;

  /* Initialize local variables.  */
  grouping = (const char *) -1;
#ifdef __va_copy
  /* This macro will be available soon in gcc's <stdarg.h>.  We need it
     since on some systems `va_list' is not an integral type.  */
  __va_copy (ap_save, ap);
#else
  ap_save = ap;
#endif
  nspecs_done = 0;

#ifdef COMPILE_WPRINTF
  /* Find the first format specifier.  */
  f = lead_str_end = __find_specwc ((const UCHAR_T *) format);
#else
  /* Find the first format specifier.  */
  f = lead_str_end = __find_specmb ((const UCHAR_T *) format);
#endif

  /* Write the literal text before the first format.  */
  Xprintf_buffer_write (buf, format,
			  lead_str_end - (const UCHAR_T *) format);
  if (Xprintf_buffer_has_failed (buf))
    return;

  /* If we only have to print a simple string, return now.  */
  if (*f == L_('\0'))
    return;

  /* Use the slow path in case any printf handler is registered.  */
  if (__glibc_unlikely (__printf_function_table != NULL
			|| __printf_modifier_table != NULL
			|| __printf_va_arg_table != NULL))
    goto do_positional;

  /* Process whole format string.  */
  do
    {
      STEP0_3_TABLE;
      STEP4_TABLE;

      int is_negative;	/* Flag for negative number.  */
      union
      {
	unsigned long long int longlong;
	unsigned long int word;
      } number;
      int base;
      union printf_arg the_arg;
      CHAR_T *string;	/* Pointer to argument string.  */
      int alt = 0;	/* Alternate format.  */
      int space = 0;	/* Use space prefix if no sign is needed.  */
      int left = 0;	/* Left-justify output.  */
      int showsign = 0;	/* Always begin with plus or minus sign.  */
      int group = 0;	/* Print numbers according grouping rules.  */
      /* Argument is long double/long long int.  Only used if
	 double/long double or long int/long long int are distinct.  */
      int is_long_double __attribute__ ((unused)) = 0;
      int is_short = 0;	/* Argument is short int.  */
      int is_long = 0;	/* Argument is long int.  */
      int is_char = 0;	/* Argument is promoted (unsigned) char.  */
      int width = 0;	/* Width of output; 0 means none specified.  */
      int prec = -1;	/* Precision of output; -1 means none specified.  */
      /* This flag is set by the 'I' modifier and selects the use of the
	 `outdigits' as determined by the current locale.  */
      int use_outdigits = 0;
      UCHAR_T pad = L_(' ');/* Padding character.  */
      CHAR_T spec;

      workend = work_buffer + WORK_BUFFER_SIZE;

      /* Get current character in format string.  */
      JUMP (*++f, step0_jumps);

      /* ' ' flag.  */
    LABEL (flag_space):
      space = 1;
      JUMP (*++f, step0_jumps);

      /* '+' flag.  */
    LABEL (flag_plus):
      showsign = 1;
      JUMP (*++f, step0_jumps);

      /* The '-' flag.  */
    LABEL (flag_minus):
      left = 1;
      pad = L_(' ');
      JUMP (*++f, step0_jumps);

      /* The '#' flag.  */
    LABEL (flag_hash):
      alt = 1;
      JUMP (*++f, step0_jumps);

      /* The '0' flag.  */
    LABEL (flag_zero):
      if (!left)
	pad = L_('0');
      JUMP (*++f, step0_jumps);

      /* The '\'' flag.  */
    LABEL (flag_quote):
      group = 1;

      if (grouping == (const char *) -1)
	{
#ifdef COMPILE_WPRINTF
	  thousands_sep = _NL_CURRENT_WORD (LC_NUMERIC,
					    _NL_NUMERIC_THOUSANDS_SEP_WC);
#else
	  thousands_sep = _NL_CURRENT (LC_NUMERIC, THOUSANDS_SEP);
#endif

	  grouping = _NL_CURRENT (LC_NUMERIC, GROUPING);
	  if (*grouping == '\0' || *grouping == CHAR_MAX
#ifdef COMPILE_WPRINTF
	      || thousands_sep == L'\0'
#else
	      || *thousands_sep == '\0'
#endif
	      )
	    grouping = NULL;
	}
      JUMP (*++f, step0_jumps);

    LABEL (flag_i18n):
      use_outdigits = 1;
      JUMP (*++f, step0_jumps);

      /* Get width from argument.  */
    LABEL (width_asterics):
      {
	const UCHAR_T *tmp;	/* Temporary value.  */

	tmp = ++f;
	if (ISDIGIT (*tmp))
	  {
	    int pos = read_int (&tmp);

	    if (pos == -1)
	      {
		__set_errno (EOVERFLOW);
		Xprintf_buffer_mark_failed (buf);
		goto all_done;
	      }

	    if (pos && *tmp == L_('$'))
	      /* The width comes from a positional parameter.  */
	      goto do_positional;
	  }
	width = va_arg (ap, int);

	/* Negative width means left justified.  */
	if (width < 0)
	  {
	    width = -width;
	    pad = L_(' ');
	    left = 1;
	  }
      }
      JUMP (*f, step1_jumps);

      /* Given width in format string.  */
    LABEL (width):
      width = read_int (&f);

      if (__glibc_unlikely (width == -1))
	{
	  __set_errno (EOVERFLOW);
	  Xprintf_buffer_mark_failed (buf);
	  goto all_done;
	}

      if (*f == L_('$'))
	/* Oh, oh.  The argument comes from a positional parameter.  */
	goto do_positional;
      JUMP (*f, step1_jumps);

    LABEL (precision):
      ++f;
      if (*f == L_('*'))
	{
	  const UCHAR_T *tmp;	/* Temporary value.  */

	  tmp = ++f;
	  if (ISDIGIT (*tmp))
	    {
	      int pos = read_int (&tmp);

	      if (pos == -1)
		{
		  __set_errno (EOVERFLOW);
		  Xprintf_buffer_mark_failed (buf);
		  goto all_done;
		}

	      if (pos && *tmp == L_('$'))
		/* The precision comes from a positional parameter.  */
		goto do_positional;
	    }
	  prec = va_arg (ap, int);

	  /* If the precision is negative the precision is omitted.  */
	  if (prec < 0)
	    prec = -1;
	}
      else if (ISDIGIT (*f))
	{
	  prec = read_int (&f);

	  /* The precision was specified in this case as an extremely
	     large positive value.  */
	  if (prec == -1)
	    {
	      __set_errno (EOVERFLOW);
	      Xprintf_buffer_mark_failed (buf);
	      goto all_done;
	    }
	}
      else
	prec = 0;
      JUMP (*f, step2_jumps);

      /* Process 'h' modifier.  There might another 'h' following.  */
    LABEL (mod_half):
      is_short = 1;
      JUMP (*++f, step3a_jumps);

      /* Process 'hh' modifier.  */
    LABEL (mod_halfhalf):
      is_short = 0;
      is_char = 1;
      JUMP (*++f, step4_jumps);

      /* Process 'l' modifier.  There might another 'l' following.  */
    LABEL (mod_long):
      is_long = 1;
      JUMP (*++f, step3b_jumps);

      /* Process 'L', 'q', or 'll' modifier.  No other modifier is
	 allowed to follow.  */
    LABEL (mod_longlong):
      is_long_double = 1;
      is_long = 1;
      JUMP (*++f, step4_jumps);

    LABEL (mod_size_t):
      is_long_double = sizeof (size_t) > sizeof (unsigned long int);
      is_long = sizeof (size_t) > sizeof (unsigned int);
      JUMP (*++f, step4_jumps);

    LABEL (mod_ptrdiff_t):
      is_long_double = sizeof (ptrdiff_t) > sizeof (unsigned long int);
      is_long = sizeof (ptrdiff_t) > sizeof (unsigned int);
      JUMP (*++f, step4_jumps);

    LABEL (mod_intmax_t):
      is_long_double = sizeof (intmax_t) > sizeof (unsigned long int);
      is_long = sizeof (intmax_t) > sizeof (unsigned int);
      JUMP (*++f, step4_jumps);

      /* Process 'wN' or 'wfN' modifier.  */
    LABEL (mod_bitwidth):
      ++f;
      bool is_fast = false;
      if (*f == L_('f'))
	{
	  ++f;
	  is_fast = true;
	}
      int bitwidth = 0;
      if (ISDIGIT (*f))
	bitwidth = read_int (&f);
      if (is_fast)
	switch (bitwidth)
	  {
	  case 8:
	    bitwidth = INT_FAST8_WIDTH;
	    break;
	  case 16:
	    bitwidth = INT_FAST16_WIDTH;
	    break;
	  case 32:
	    bitwidth = INT_FAST32_WIDTH;
	    break;
	  case 64:
	    bitwidth = INT_FAST64_WIDTH;
	    break;
	  }
      switch (bitwidth)
	{
	case 8:
	  is_char = 1;
	  break;
	case 16:
	  is_short = 1;
	  break;
	case 32:
	  break;
	case 64:
	  is_long_double = 1;
	  is_long = 1;
	  break;
	default:
	  /* ISO C requires this error to be detected.  */
	  __set_errno (EINVAL);
	  Xprintf_buffer_mark_failed (buf);
	  goto all_done;
	}
      JUMP (*f, step4_jumps);

      /* Process current format.  */
      while (1)
	{
#define process_arg_int() va_arg (ap, int)
#define process_arg_long_int() va_arg (ap, long int)
#define process_arg_long_long_int() va_arg (ap, long long int)
#define process_arg_pointer() va_arg (ap, void *)
#define process_arg_string() va_arg (ap, const char *)
#define process_arg_unsigned_int() va_arg (ap, unsigned int)
#define process_arg_unsigned_long_int() va_arg (ap, unsigned long int)
#define process_arg_unsigned_long_long_int() va_arg (ap, unsigned long long int)
#define process_arg_wchar_t() va_arg (ap, wchar_t)
#define process_arg_wstring() va_arg (ap, const wchar_t *)
#include "vfprintf-process-arg.c"
#undef process_arg_int
#undef process_arg_long_int
#undef process_arg_long_long_int
#undef process_arg_pointer
#undef process_arg_string
#undef process_arg_unsigned_int
#undef process_arg_unsigned_long_int
#undef process_arg_unsigned_long_long_int
#undef process_arg_wchar_t
#undef process_arg_wstring

	LABEL (form_float):
	LABEL (form_floathex):
	  {
	    if (__glibc_unlikely ((mode_flags & PRINTF_LDBL_IS_DBL) != 0))
	      is_long_double = 0;

	    struct printf_info info =
	      {
		.prec = prec,
		.width = width,
		.spec = spec,
		.is_long_double = is_long_double,
		.is_short = is_short,
		.is_long = is_long,
		.alt = alt,
		.space = space,
		.left = left,
		.showsign = showsign,
		.group = group,
		.pad = pad,
		.extra = 0,
		.i18n = use_outdigits,
		.wide = sizeof (CHAR_T) != 1,
		.is_binary128 = 0
	      };

	    PARSE_FLOAT_VA_ARG_EXTENDED (info);
	    const void *ptr = &the_arg;

	    __printf_fp_spec (buf, &info, &ptr);
	  }
	  break;

	LABEL (form_unknown):
	  if (spec == L_('\0'))
	    {
	      /* The format string ended before the specifier is complete.  */
	      __set_errno (EINVAL);
	      Xprintf_buffer_mark_failed (buf);
	      goto all_done;
	    }

	  /* If we are in the fast loop force entering the complicated
	     one.  */
	  goto do_positional;
	}

      /* The format is correctly handled.  */
      ++nspecs_done;

      /* Look for next format specifier.  */
#ifdef COMPILE_WPRINTF
      f = __find_specwc ((end_of_spec = ++f));
#else
      f = __find_specmb ((end_of_spec = ++f));
#endif

      /* Write the following constant string.  */
      Xprintf_buffer_write (buf, (const CHAR_T *) end_of_spec,
			      f - end_of_spec);
    }
  while (*f != L_('\0') && !Xprintf_buffer_has_failed (buf));

 all_done:
  /* printf_positional performs cleanup under its all_done label, so
     vfprintf-process-arg.c uses it for this function and
     printf_positional below.  */
  return;

  /* Hand off processing for positional parameters.  */
do_positional:
  printf_positional (buf, format, readonly_format, ap, &ap_save,
		     nspecs_done, lead_str_end, work_buffer,
		     save_errno, grouping, thousands_sep, mode_flags);
}

static void
printf_positional (struct Xprintf_buffer * buf, const CHAR_T *format,
		   int readonly_format,
		   va_list ap, va_list *ap_savep, int nspecs_done,
		   const UCHAR_T *lead_str_end,
		   CHAR_T *work_buffer, int save_errno,
		   const char *grouping, THOUSANDS_SEP_T thousands_sep,
		   unsigned int mode_flags)
{
  /* For positional argument handling.  */
  struct scratch_buffer specsbuf;
  scratch_buffer_init (&specsbuf);
  struct printf_spec *specs = specsbuf.data;
  size_t specs_limit = specsbuf.length / sizeof (specs[0]);

  /* Used as a backing store for args_value, args_size, args_type
     below.  */
  struct scratch_buffer argsbuf;
  scratch_buffer_init (&argsbuf);

  /* Array with information about the needed arguments.  This has to
     be dynamically extensible.  */
  size_t nspecs = 0;

  /* The number of arguments the format string requests.  This will
     determine the size of the array needed to store the argument
     attributes.  */
  size_t nargs = 0;

  /* Positional parameters refer to arguments directly.  This could
     also determine the maximum number of arguments.  Track the
     maximum number.  */
  size_t max_ref_arg = 0;

  /* Just a counter.  */
  size_t cnt;

  if (grouping == (const char *) -1)
    {
#ifdef COMPILE_WPRINTF
      thousands_sep = _NL_CURRENT_WORD (LC_NUMERIC,
					_NL_NUMERIC_THOUSANDS_SEP_WC);
#else
      thousands_sep = _NL_CURRENT (LC_NUMERIC, THOUSANDS_SEP);
#endif

      grouping = _NL_CURRENT (LC_NUMERIC, GROUPING);
      if (*grouping == '\0' || *grouping == CHAR_MAX)
	grouping = NULL;
    }

  for (const UCHAR_T *f = lead_str_end; *f != L_('\0');
       f = specs[nspecs++].next_fmt)
    {
      if (nspecs == specs_limit)
	{
	  if (!scratch_buffer_grow_preserve (&specsbuf))
	    {
	      Xprintf_buffer_mark_failed (buf);
	      goto all_done;
	    }
	  specs = specsbuf.data;
	  specs_limit = specsbuf.length / sizeof (specs[0]);
	}

      /* Parse the format specifier.  */
      bool failed;
#ifdef COMPILE_WPRINTF
      nargs += __parse_one_specwc (f, nargs, &specs[nspecs], &max_ref_arg,
				   &failed);
#else
      nargs += __parse_one_specmb (f, nargs, &specs[nspecs], &max_ref_arg,
				   &failed);
#endif
      if (failed)
	{
	  Xprintf_buffer_mark_failed (buf);
	  goto all_done;
	}
    }

  /* Determine the number of arguments the format string consumes.  */
  nargs = MAX (nargs, max_ref_arg);

  union printf_arg *args_value;
  int *args_size;
  int *args_type;
  void *args_pa_user;
  size_t args_pa_user_offset;
  {
    /* Calculate total size needed to represent a single argument
       across all three argument-related arrays.  */
    size_t bytes_per_arg
      = sizeof (*args_value) + sizeof (*args_size) + sizeof (*args_type);
    if (!scratch_buffer_set_array_size (&argsbuf, nargs, bytes_per_arg))
      {
	Xprintf_buffer_mark_failed (buf);
	goto all_done;
      }
    args_value = argsbuf.data;
    /* Set up the remaining two arrays to each point past the end of
       the prior array, since space for all three has been allocated
       now.  */
    args_size = &args_value[nargs].pa_int;
    args_type = &args_size[nargs];
    args_pa_user = &args_type[nargs];
    memset (args_type, (mode_flags & PRINTF_FORTIFY) != 0 ? '\xff' : '\0',
	    nargs * sizeof (*args_type));
  }

  /* XXX Could do sanity check here: If any element in ARGS_TYPE is
     still zero after this loop, format is invalid.  For now we
     simply use 0 as the value.  */

  /* Fill in the types of all the arguments.  */
  for (cnt = 0; cnt < nspecs; ++cnt)
    {
      /* If the width is determined by an argument this is an int.  */
      if (specs[cnt].width_arg != -1)
	args_type[specs[cnt].width_arg] = PA_INT;

      /* If the precision is determined by an argument this is an int.  */
      if (specs[cnt].prec_arg != -1)
	args_type[specs[cnt].prec_arg] = PA_INT;

      switch (specs[cnt].ndata_args)
	{
	case 0:		/* No arguments.  */
	  break;
	case 1:		/* One argument; we already have the
			   type and size.  */
	  args_type[specs[cnt].data_arg] = specs[cnt].data_arg_type;
	  args_size[specs[cnt].data_arg] = specs[cnt].size;
	  break;
	default:
	  /* We have more than one argument for this format spec.
	     We must call the arginfo function again to determine
	     all the types.  */
	  (void) (*__printf_arginfo_table[specs[cnt].info.spec])
	    (&specs[cnt].info,
	     specs[cnt].ndata_args, &args_type[specs[cnt].data_arg],
	     &args_size[specs[cnt].data_arg]);
	  break;
	}
    }

  /* Now we know all the types and the order.  Fill in the argument
     values.  */
  for (cnt = 0; cnt < nargs; ++cnt)
    switch (args_type[cnt])
      {
#define T(tag, mem, type)				\
	case tag:					\
	  args_value[cnt].mem = va_arg (*ap_savep, type); \
	  break

	T (PA_WCHAR, pa_wchar, wint_t);
      case PA_CHAR:				/* Promoted.  */
      case PA_INT|PA_FLAG_SHORT:		/* Promoted.  */
#if LONG_MAX == INT_MAX
      case PA_INT|PA_FLAG_LONG:
#endif
	T (PA_INT, pa_int, int);
#if LONG_MAX == LONG_LONG_MAX
      case PA_INT|PA_FLAG_LONG:
#endif
	T (PA_INT|PA_FLAG_LONG_LONG, pa_long_long_int, long long int);
#if LONG_MAX != INT_MAX && LONG_MAX != LONG_LONG_MAX
# error "he?"
#endif
      case PA_FLOAT:				/* Promoted.  */
	T (PA_DOUBLE, pa_double, double);
      case PA_DOUBLE|PA_FLAG_LONG_DOUBLE:
	if (__glibc_unlikely ((mode_flags & PRINTF_LDBL_IS_DBL) != 0))
	  {
	    args_value[cnt].pa_double = va_arg (*ap_savep, double);
	    args_type[cnt] &= ~PA_FLAG_LONG_DOUBLE;
	  }
#if __HAVE_FLOAT128_UNLIKE_LDBL
	else if ((mode_flags & PRINTF_LDBL_USES_FLOAT128) != 0)
	  args_value[cnt].pa_float128 = va_arg (*ap_savep, _Float128);
#endif
	else
	  args_value[cnt].pa_long_double = va_arg (*ap_savep, long double);
	break;
      case PA_STRING:				/* All pointers are the same */
      case PA_WSTRING:			/* All pointers are the same */
	T (PA_POINTER, pa_pointer, void *);
#undef T
      default:
	if ((args_type[cnt] & PA_FLAG_PTR) != 0)
	  args_value[cnt].pa_pointer = va_arg (*ap_savep, void *);
	else if (__glibc_unlikely (__printf_va_arg_table != NULL)
		 && __printf_va_arg_table[args_type[cnt] - PA_LAST] != NULL)
	  {
	    while (args_pa_user + args_size[cnt] >
		argsbuf.data + argsbuf.length)
	      {
		args_pa_user_offset = args_pa_user - (void *) &args_type[nargs];
	        if (!scratch_buffer_grow_preserve (&argsbuf))
	          {
	            Xprintf_buffer_mark_failed (buf);
	            goto all_done;
	          }
                args_value = argsbuf.data;
                /* Set up the remaining two arrays to each point past the end of
                   the prior array, since space for all three has been allocated
                   now.  */
                args_size = &args_value[nargs].pa_int;
                args_type = &args_size[nargs];
                args_pa_user = (void *) &args_type[nargs] + args_pa_user_offset;
	      }
	    args_value[cnt].pa_user = args_pa_user;
	    args_pa_user += args_size[cnt];
	    (*__printf_va_arg_table[args_type[cnt] - PA_LAST])
	      (args_value[cnt].pa_user, ap_savep);
	  }
	else
	  memset (&args_value[cnt], 0, sizeof (args_value[cnt]));
	break;
      case -1:
	/* Error case.  Not all parameters appear in N$ format
	   strings.  We have no way to determine their type.  */
	assert ((mode_flags & PRINTF_FORTIFY) != 0);
	__libc_fatal ("*** invalid %N$ use detected ***\n");
      }

  /* Now walk through all format specifiers and process them.  */
  for (; (size_t) nspecs_done < nspecs && !Xprintf_buffer_has_failed (buf);
       ++nspecs_done)
    {
      STEP4_TABLE;

      int is_negative;
      union
      {
	unsigned long long int longlong;
	unsigned long int word;
      } number;
      int base;
      CHAR_T *string;		/* Pointer to argument string.  */

      /* Fill variables from values in struct.  */
      int alt = specs[nspecs_done].info.alt;
      int space = specs[nspecs_done].info.space;
      int left = specs[nspecs_done].info.left;
      int showsign = specs[nspecs_done].info.showsign;
      int group = specs[nspecs_done].info.group;
      int is_long_double __attribute__ ((unused))
	= specs[nspecs_done].info.is_long_double;
      int is_short = specs[nspecs_done].info.is_short;
      int is_char = specs[nspecs_done].info.is_char;
      int is_long = specs[nspecs_done].info.is_long;
      int width = specs[nspecs_done].info.width;
      int prec = specs[nspecs_done].info.prec;
      int use_outdigits = specs[nspecs_done].info.i18n;
      char pad = specs[nspecs_done].info.pad;
      CHAR_T spec = specs[nspecs_done].info.spec;

      CHAR_T *workend = work_buffer + WORK_BUFFER_SIZE;

      /* Fill in last information.  */
      if (specs[nspecs_done].width_arg != -1)
	{
	  /* Extract the field width from an argument.  */
	  specs[nspecs_done].info.width =
	    args_value[specs[nspecs_done].width_arg].pa_int;

	  if (specs[nspecs_done].info.width < 0)
	    /* If the width value is negative left justification is
	       selected and the value is taken as being positive.  */
	    {
	      specs[nspecs_done].info.width *= -1;
	      left = specs[nspecs_done].info.left = 1;
	    }
	  width = specs[nspecs_done].info.width;
	}

      if (specs[nspecs_done].prec_arg != -1)
	{
	  /* Extract the precision from an argument.  */
	  specs[nspecs_done].info.prec =
	    args_value[specs[nspecs_done].prec_arg].pa_int;

	  if (specs[nspecs_done].info.prec < 0)
	    /* If the precision is negative the precision is
	       omitted.  */
	    specs[nspecs_done].info.prec = -1;

	  prec = specs[nspecs_done].info.prec;
	}

      /* Process format specifiers.  */
      do
	{
# ifdef COMPILE_WPRINTF
#  define CHECK_SPEC(spec) ((spec) <= UCHAR_MAX)
# else
#  define CHECK_SPEC(spec) (true)
# endif
	  if (CHECK_SPEC (spec)
	      && __printf_function_table != NULL
	      && __printf_function_table[(size_t) spec] != NULL)
	    {
	      int function_done
		= Xprintf (function_invoke) (buf,
					     __printf_function_table[(size_t) spec],
					     &args_value[specs[nspecs_done]
							 .data_arg],
					     specs[nspecs_done].ndata_args,
					     &specs[nspecs_done].info);
	      if (function_done != -2)
		{
		  /* If an error occurred we don't have information
		     about # of chars.  */
		  if (function_done < 0)
		    {
		      /* Function has set errno.  */
		      Xprintf_buffer_mark_failed (buf);
		      goto all_done;
		    }
		  break;
		}
	    }

	  JUMP (spec, step4_jumps);

#define process_arg_data args_value[specs[nspecs_done].data_arg]
#define process_arg_int() process_arg_data.pa_int
#define process_arg_long_int() process_arg_data.pa_long_int
#define process_arg_long_long_int() process_arg_data.pa_long_long_int
#define process_arg_pointer() process_arg_data.pa_pointer
#define process_arg_string() process_arg_data.pa_string
#define process_arg_unsigned_int() process_arg_data.pa_u_int
#define process_arg_unsigned_long_int() process_arg_data.pa_u_long_int
#define process_arg_unsigned_long_long_int() process_arg_data.pa_u_long_long_int
#define process_arg_wchar_t() process_arg_data.pa_wchar
#define process_arg_wstring() process_arg_data.pa_wstring
#include "vfprintf-process-arg.c"
#undef process_arg_data
#undef process_arg_int
#undef process_arg_long_int
#undef process_arg_long_long_int
#undef process_arg_pointer
#undef process_arg_string
#undef process_arg_unsigned_int
#undef process_arg_unsigned_long_int
#undef process_arg_unsigned_long_long_int
#undef process_arg_wchar_t
#undef process_arg_wstring

	  LABEL (form_float):
	  LABEL (form_floathex):
	  {
	    const void *ptr
	      = (const void *) &args_value[specs[nspecs_done].data_arg];
	    if (__glibc_unlikely ((mode_flags & PRINTF_LDBL_IS_DBL) != 0))
	      {
		specs[nspecs_done].data_arg_type = PA_DOUBLE;
		specs[nspecs_done].info.is_long_double = 0;
	      }
	    SETUP_FLOAT128_INFO (specs[nspecs_done].info);

	    __printf_fp_spec (buf, &specs[nspecs_done].info, &ptr);
	  }
	  break;

	  LABEL (form_unknown):
	  {
	    printf_unknown (buf, &specs[nspecs_done].info);
	  }
	  break;
	}
      while (Xprintf_buffer_has_failed (buf));

      /* Write the following constant string.  */
      Xprintf_buffer_write (buf,
			      (const CHAR_T *) specs[nspecs_done].end_of_fmt,
			      (specs[nspecs_done].next_fmt
			       - specs[nspecs_done].end_of_fmt));
    }
 all_done:
  scratch_buffer_free (&argsbuf);
  scratch_buffer_free (&specsbuf);
}

/* Handle an unknown format specifier.  This prints out a canonicalized
   representation of the format spec itself.  */
static void
printf_unknown (struct Xprintf_buffer *buf, const struct printf_info *info)
{
  CHAR_T work_buffer[MAX (sizeof (info->width), sizeof (info->prec)) * 3];
  CHAR_T *const workend
    = &work_buffer[sizeof (work_buffer) / sizeof (CHAR_T)];
  CHAR_T *w;

  Xprintf_buffer_putc (buf, L_('%'));

  if (info->alt)
    Xprintf_buffer_putc (buf, L_('#'));
  if (info->group)
    Xprintf_buffer_putc (buf, L_('\''));
  if (info->showsign)
    Xprintf_buffer_putc (buf, L_('+'));
  else if (info->space)
    Xprintf_buffer_putc (buf, L_(' '));
  if (info->left)
    Xprintf_buffer_putc (buf, L_('-'));
  if (info->pad == L_('0'))
    Xprintf_buffer_putc (buf, L_('0'));
  if (info->i18n)
    Xprintf_buffer_putc (buf, L_('I'));

  if (info->width != 0)
    {
      w = _itoa_word (info->width, workend, 10, 0);
      while (w < workend)
	Xprintf_buffer_putc (buf, *w++);
    }

  if (info->prec != -1)
    {
      Xprintf_buffer_putc (buf, L_('.'));
      w = _itoa_word (info->prec, workend, 10, 0);
      while (w < workend)
	Xprintf_buffer_putc (buf, *w++);
    }

  if (info->spec != L_('\0'))
    Xprintf_buffer_putc (buf, info->spec);
}

static void
group_number (struct Xprintf_buffer *buf,
	      struct grouping_iterator *iter,
	      CHAR_T *from, CHAR_T *to, THOUSANDS_SEP_T thousands_sep,
	      bool i18n)
{
  if (!i18n)
    for (CHAR_T *cp = from; cp != to; ++cp)
      {
	if (__grouping_iterator_next (iter))
	  {
#ifdef COMPILE_WPRINTF
	    __wprintf_buffer_putc (buf, thousands_sep);
#else
	    __printf_buffer_puts (buf, thousands_sep);
#endif
	  }
	Xprintf_buffer_putc (buf, *cp);
      }
  else
    {
      /* Apply digit translation and grouping.  */
      for (CHAR_T *cp = from; cp != to; ++cp)
	{
	  if (__grouping_iterator_next (iter))
	    {
#ifdef COMPILE_WPRINTF
	      __wprintf_buffer_putc (buf, thousands_sep);
#else
	      __printf_buffer_puts (buf, thousands_sep);
#endif
	    }
	  int digit = *cp - '0';
#ifdef COMPILE_WPRINTF
	  __wprintf_buffer_putc
	    (buf, _NL_CURRENT_WORD (LC_CTYPE,
				    _NL_CTYPE_OUTDIGIT0_WC + digit));
#else
	  __printf_buffer_puts
	    (buf, _NL_CURRENT (LC_CTYPE, _NL_CTYPE_OUTDIGIT0_MB + digit));
#endif
	}
    }
}


/* The FILE-based function.  */
int
vfprintf (FILE *s, const CHAR_T *format, va_list ap, unsigned int mode_flags)
{
  /* Orient the stream.  */
#ifdef ORIENT
  ORIENT;
#endif

  /* Sanity check of arguments.  */
  ARGCHECK (s, format);

#ifdef ORIENT
  /* Check for correct orientation.  */
  if (_IO_vtable_offset (s) == 0
      && _IO_fwide (s, sizeof (CHAR_T) == 1 ? -1 : 1)
      != (sizeof (CHAR_T) == 1 ? -1 : 1))
    /* The stream is already oriented otherwise.  */
    return EOF;
#endif

  if (!_IO_need_lock (s))
    {
      struct Xprintf (buffer_to_file) wrap;
      Xprintf (buffer_to_file_init) (&wrap, s);
      Xprintf_buffer (&wrap.base, format, ap, mode_flags);
      return Xprintf (buffer_to_file_done) (&wrap);
    }

  int done;

  /* Lock stream.  */
  _IO_cleanup_region_start ((void (*) (void *)) &_IO_funlockfile, s);
  _IO_flockfile (s);

  /* Set up the wrapping buffer.  */
  struct Xprintf (buffer_to_file) wrap;
  Xprintf (buffer_to_file_init) (&wrap, s);

  /* Perform the printing operation on the buffer.  */
  Xprintf_buffer (&wrap.base, format, ap, mode_flags);
  done = Xprintf (buffer_to_file_done) (&wrap);

  /* Unlock the stream.  */
  _IO_funlockfile (s);
  _IO_cleanup_region_end (0);

  return done;
}
