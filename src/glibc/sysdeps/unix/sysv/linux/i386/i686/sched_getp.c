#include <unistd.h>
#include <sys/types.h>
#include <sched.h>
#include <lind_debug.h>

int
__sched_getparam (pid_t pid, struct sched_param *param)
{
  lind_debug_panic("sched_getparam called but not supported!");
  return -1;
}
libc_hidden_def (__sched_getparam)
stub_warning (sched_getparam)

weak_alias (__sched_getparam, sched_getparam)
