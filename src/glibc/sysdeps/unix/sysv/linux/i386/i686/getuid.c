#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

__uid_t
__getuid (void)
{
  return MAKE_TRANDITION(GETUID_SYSCALL, "syscall|getuid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

weak_alias(__getuid, getuid)
