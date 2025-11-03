#include <unistd.h>
#include <sys/types.h>
#include <sched.h>

int
__sched_getparam (pid_t pid, struct sched_param *param)
{
  return -1;
}
