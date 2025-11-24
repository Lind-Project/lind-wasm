#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

gid_t
__getgid (void)
{
  return MAKE_LEGACY_SYSCALL(GETGID_SYSCALL, "syscall|getgid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

weak_alias(__getgid, getgid)
