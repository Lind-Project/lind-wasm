#include <unistd.h>
#include <syscall-template.h>

__uid_t
__getuid (void)
{
  return MAKE_SYSCALL(50, "syscall|getuid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias(__getuid, getuid)
