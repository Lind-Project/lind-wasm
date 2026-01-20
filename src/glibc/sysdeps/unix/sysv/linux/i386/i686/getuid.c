#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

__uid_t
__getuid (void)
{
  return MAKE_LEGACY_SYSCALL(GETUID_SYSCALL, "syscall|getuid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

weak_alias(__getuid, getuid)
