#include <unistd.h>
#include <sched.h>
#include <sys/types.h>

int
__sched_setscheduler (pid_t pid, int policy, const struct sched_param *param)
{
  return -1;
}

