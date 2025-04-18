/* setitimer -- Set the state of an interval timer.  Linux/32 version.
   Copyright (C) 2020-2024 Free Software Foundation, Inc.
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
   <http://www.gnu.org/licenses/>.  */

#include <time.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sysdep.h>
#include <tv32-compat.h>
#include <syscall-template.h>

int
__setitimer64 (__itimer_which_t which,
               const struct __itimerval64 *restrict new_value,
               struct __itimerval64 *restrict old_value)
{
  return MAKE_SYSCALL(150, "syscall|setitimer", (uint64_t) which, (uint64_t) new_value, (uint64_t) old_value, NOTUSED, NOTUSED, NOTUSED);
}

#if __TIMESIZE != 64
libc_hidden_def (__setitimer64)
int
__setitimer (__itimer_which_t which,
             const struct itimerval *restrict new_value,
             struct itimerval *restrict old_value)
{
  int ret;
  struct __itimerval64 new64, old64;

  new64.it_interval
    = valid_timeval_to_timeval64 (new_value->it_interval);
  new64.it_value
    = valid_timeval_to_timeval64 (new_value->it_value);

  ret = __setitimer64 (which, &new64, old_value ? &old64 : NULL);

  if (ret == 0 && old_value != NULL)
    {
      old_value->it_interval
        = valid_timeval64_to_timeval (old64.it_interval);
      old_value->it_value
        = valid_timeval64_to_timeval (old64.it_value);
    }

  return ret;
}
#endif
weak_alias (__setitimer, setitimer)
