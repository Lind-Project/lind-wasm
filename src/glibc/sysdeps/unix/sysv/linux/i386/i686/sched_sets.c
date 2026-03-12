#include <unistd.h>
#include <sched.h>
#include <sys/types.h>
#include <lind_debug.h>

int
__sched_setscheduler (pid_t pid, int policy, const struct sched_param *param)
{
  lind_debug_panic("sched_setscheduler called but not supported!");
  return -1;
}
libc_hidden_def (__sched_setscheduler)
stub_warning (sched_setscheduler)

weak_alias (__sched_setscheduler, sched_setscheduler)
