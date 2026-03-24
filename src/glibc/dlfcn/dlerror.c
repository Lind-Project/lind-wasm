/* Return error detail for failing <dlfcn.h> functions.
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

#include <dlfcn.h>
#include <libintl.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libc-lock.h>
#include <ldsodefs.h>
#include <libc-symbols.h>
#include <assert.h>
#include <dlerror.h>

// error message for lind dynamic loader
// dylink errno returned from host is served as the index to the array
// should match sysdefs/lind_platform_const.rs
static const char *__lind_dylink_error_msg[] = {
    "UNUSED", // 0
    "cannot open shared object file",
    "Invalid library file type: expects wasm file",
    "shared wasm object file does not have dylink.0 section",
    "error while loading shared libraries",
    "undefined symbol",

    "invalid handler"
    "symbol not found",

    "shared object is already closed",

    "internal error",
};

char *
__dlerror (void)
{
  if (__lind_dlerror_result != 0) {
    size_t len = sizeof(__lind_dylink_error_msg) / sizeof(__lind_dylink_error_msg[0]);
    assert(__lind_dlerror_result < len);
    
    char* msg = __lind_dylink_error_msg[__lind_dlerror_result];
    __lind_dlerror_result = 0; // clear the error message

    return msg;
  }

  return NULL;
}
versioned_symbol (libc, __dlerror, dlerror, GLIBC_2_34);

#if OTHER_SHLIB_COMPAT (libdl, GLIBC_2_0, GLIBC_2_34)
compat_symbol (libdl, __dlerror, dlerror, GLIBC_2_0);
#endif

int
_dlerror_run (void (*operate) (void *), void *args)
{
  struct dl_action_result *result = __libc_dlerror_result;
  if (result != NULL)
    {
      if (result == dl_action_result_malloc_failed)
	{
	  /* Clear the previous error.  */
	  __libc_dlerror_result = NULL;
	  result = NULL;
	}
      else
	{
	  /* There is an existing object.  Free its error string, but
	     keep the object.  */
	  dl_action_result_errstring_free (result);
	  /* Mark the object as not containing an error.  This ensures
	     that call to dlerror from, for example, an ELF
	     constructor will not notice this result object.  */
	  result->errstring = NULL;
	}
    }

  const char *objname;
  const char *errstring;
  bool malloced;
  int errcode = GLRO (dl_catch_error) (&objname, &errstring, &malloced,
				       operate, args);

  /* ELF constructors or destructors may have indirectly altered the
     value of __libc_dlerror_result, therefore reload it.  */
  result = __libc_dlerror_result;

  if (errstring == NULL)
    {
      /* There is no error.  We no longer need the result object if it
	 does not contain an error.  However, a recursive call may
	 have added an error even if this call did not cause it.  Keep
	 the other error.  */
      if (result != NULL && result->errstring == NULL)
	{
	  __libc_dlerror_result = NULL;
	  free (result);
	}
      return 0;
    }
  else
    {
      /* A new error occurred.  Check if a result object has to be
	 allocated.  */
      if (result == NULL || result == dl_action_result_malloc_failed)
	{
	  /* Allocating storage for the error message after the fact
	     is not ideal.  But this avoids an infinite recursion in
	     case malloc itself calls libdl functions (without
	     triggering errors).  */
	  result = malloc (sizeof (*result));
	  if (result == NULL)
	    {
	      /* Assume that the dlfcn failure was due to a malloc
		 failure, too.  */
	      if (malloced)
		dl_error_free ((char *) errstring);
	      __libc_dlerror_result = dl_action_result_malloc_failed;
	      return 1;
	    }
	  __libc_dlerror_result = result;
	}
      else
	/* Deallocate the existing error message from a recursive
	   call, but reuse the result object.  */
	dl_action_result_errstring_free (result);

      result->errcode = errcode;
      result->objname = objname;
      result->errstring = (char *) errstring;
      result->returned = false;
      /* In case of an error, the malloced flag indicates whether the
	 error string is constant or not.  */
      if (malloced)
	result->errstring_source = dl_action_result_errstring_rtld;
      else
	result->errstring_source = dl_action_result_errstring_constant;

      return 1;
    }
}
