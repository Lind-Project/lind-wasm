#include <unistd.h>
#include <lind_debug.h>

int
__GI___sched_get_priority_max (int __algorithm)
{
  lind_debug_panic("sched_get_priority_max called but not supported!");
  return 0;
}

weak_alias(__GI___sched_get_priority_max, sched_get_priority_max)
weak_alias(__GI___sched_get_priority_max, __sched_get_priority_max)
