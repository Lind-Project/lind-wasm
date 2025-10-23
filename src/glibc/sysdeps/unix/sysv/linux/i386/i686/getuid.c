#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

__uid_t
__getuid (void)
{
  return MAKE_SYSCALL0(GETUID_SYSCALL, "syscall|getuid");
}

weak_alias(__getuid, getuid)
