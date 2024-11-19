#include <unistd.h>
#include <stdarg.h>

int
__GI___prctl (int option, ...)
{
  return 0;
}

int
prctl (int option, ...)
{
  return 0;
}

int
__prctl_time64 (int option, ...)
{
  return 0;
}

weak_alias(__GI___prctl, __prctl)

