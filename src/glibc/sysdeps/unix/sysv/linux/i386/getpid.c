#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

pid_t
__getpid (void)
{
  return MAKE_TRADITION(GETPID_SYSCALL, "syscall|getpid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

pid_t
getpid (void)
{
  return __getpid();
}
