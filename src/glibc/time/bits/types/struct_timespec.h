/* NB: Include guard matches what <linux/time.h> uses.  */
#ifndef _STRUCT_TIMESPEC
#define _STRUCT_TIMESPEC 1

#include <bits/types.h>
#include <bits/endian.h>
#include <bits/types/time_t.h>

/* POSIX.1b structure for a time value.  This is like a `struct timeval' but
   has nanoseconds instead of microseconds.  */
  // Lind-Wasm: Original glibc code removed for compatibility
  // to find original source code refer to (2.39.9000) at (time/bits/types/struct_timespec.h):(LINE 11-33)

struct timespec
{
  time_t tv_sec;		/* Seconds.  */
  __int32_t __padding;           /* Padding.  */
  long int tv_nsec;  /* Nanoseconds.  */
  __int32_t __padding2;           /* Padding.  */
};

#endif
