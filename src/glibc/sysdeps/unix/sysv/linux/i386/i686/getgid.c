#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

gid_t
__getgid (void)
{
  return MAKE_SYSCALL0 (GETGID_SYSCALL, "syscall|getgid");
}

weak_alias (__getgid, getgid)
