#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__fchdir (int __fd)
{
  return MAKE_LEGACY_SYSCALL(FCHDIR_SYSCALL, "syscall|fchdir", (uint64_t) __fd, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

weak_alias (__fchdir, fchdir)
