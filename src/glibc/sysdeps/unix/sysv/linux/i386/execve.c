#include <unistd.h>
#include <syscall-template.h>

int __execve (const char *__path, char *const __argv[], char *const __envp[])
{

  return MAKE_SYSCALL(69, "syscall|execve", __path, __argv, __envp, NOTUSED, NOTUSED, NOTUSED);
}
strong_alias (__execve, execve)
