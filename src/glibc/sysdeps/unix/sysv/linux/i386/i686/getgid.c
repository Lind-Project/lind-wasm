#include <unistd.h>
#include <syscall-template.h>

gid_t
__getgid (void)
{
  return MAKE_SYSCALL(52, "syscall|getgid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias(__getgid, getgid)
