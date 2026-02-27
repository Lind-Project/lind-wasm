#include <unistd.h>
#include <lind_debug.h>

int
__GI___sched_get_priority_min (int __algorithm)
{
  lind_debug_panic("sched_get_priority_min called but not supported!");
  return 0;
}

weak_alias(__GI___sched_get_priority_min, __sched_get_priority_min)
weak_alias(__GI___sched_get_priority_min, sched_get_priority_min)
