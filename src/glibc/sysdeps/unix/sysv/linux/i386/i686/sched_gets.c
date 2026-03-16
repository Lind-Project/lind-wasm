#include <unistd.h>
#include <lind_debug.h>

int
__GI___sched_getscheduler (__pid_t __pid)
{
  lind_debug_panic("sched_getscheduler called but not supported!");
  return -1;
}

weak_alias(__GI___sched_getscheduler, __sched_getscheduler)
weak_alias(__GI___sched_getscheduler, sched_getscheduler)
