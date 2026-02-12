#include <unistd.h>
#include <sched.h>
#include <sys/types.h>

int
__sched_setscheduler (pid_t pid, int policy, const struct sched_param *param)
{
  return -1;
}
libc_hidden_def (__sched_setscheduler)
stub_warning (sched_setscheduler)

weak_alias (__sched_setscheduler, sched_setscheduler)
