#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

pid_t
__getpid (void)
{
  return MAKE_LEGACY_SYSCALL(GETPID_SYSCALL, "syscall|getpid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

pid_t
getpid (void)
{
  return __getpid();
}
