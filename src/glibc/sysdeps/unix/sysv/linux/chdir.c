#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__chdir (const char *__path)
{
  return MAKE_SYSCALL (CHDIR_SYSCALL, "syscall|chdir", (uint64_t) __path,
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

int
chdir (const char *__path)
{
  return MAKE_SYSCALL (CHDIR_SYSCALL, "syscall|chdir", (uint64_t) __path,
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
