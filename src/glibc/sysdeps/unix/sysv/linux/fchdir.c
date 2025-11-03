#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__fchdir (int __fd)
{
  return MAKE_SYSCALL (FCHDIR_SYSCALL, "syscall|fchdir", (uint64_t) __fd,
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias (__fchdir, fchdir)
