#include <unistd.h>
#include <sys/types.h>
#include <sched.h>

int
__sched_setparam (pid_t pid, const struct sched_param *param)
{
  return -1;
}


