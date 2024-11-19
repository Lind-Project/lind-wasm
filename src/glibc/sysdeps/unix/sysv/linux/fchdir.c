#include <unistd.h>
#include <syscall-template.h>

int
__fchdir (int __fd)
{
  return MAKE_SYSCALL(161, "syscall|fchdir", (uint64_t) __fd, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias (__fchdir, fchdir)
