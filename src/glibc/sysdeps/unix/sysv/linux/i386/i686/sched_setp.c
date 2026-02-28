#include <unistd.h>
#include <sys/types.h>
#include <sched.h>
#include <lind_debug.h>

int
__sched_setparam (pid_t pid, const struct sched_param *param)
{
  lind_debug_panic("sched_setparam called but not supported!");
  return -1;
}
libc_hidden_def (__sched_setparam)
stub_warning (sched_setparam)

weak_alias (__sched_setparam, sched_setparam)
