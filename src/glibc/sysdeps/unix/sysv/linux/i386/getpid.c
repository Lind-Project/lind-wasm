#include <unistd.h>
#include <syscall-template.h>

pid_t
__getpid (void)
{
  return MAKE_SYSCALL(31, "syscall|getpid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

pid_t
getpid (void)
{
  return __getpid();
}
