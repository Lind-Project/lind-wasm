#include <unistd.h>
#include <sys/time.h>

unsigned int
alarm (unsigned int seconds)
{
  // lind-wasm: implementation copied from sysdeps/posix/alarm.c
  struct itimerval old, new;
  unsigned int retval;

  new.it_interval.tv_usec = 0;
  new.it_interval.tv_sec = 0;
  new.it_value.tv_usec = 0;
  new.it_value.tv_sec = (long int) seconds;
  if (__setitimer (ITIMER_REAL, &new, &old) < 0)
    return 0;

  retval = old.it_value.tv_sec;
  /* Round to the nearest second, but never report zero seconds when
     the alarm is still set.  */
  if (old.it_value.tv_usec >= 500000
      || (retval == 0 && old.it_value.tv_usec > 0))
    ++retval;
  return retval;
}
