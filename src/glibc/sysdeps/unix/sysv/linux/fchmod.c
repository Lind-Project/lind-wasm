#include <unistd.h>
#include <stdarg.h>
#include <stddef.h>
#include <sys/stat.h>
#include <syscall-template.h>

int
fchmod (int fd, mode_t mode)
{
  return MAKE_SYSCALL(134, "syscall|fchmod", (uint64_t) fd, (uint64_t) mode, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

