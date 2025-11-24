#include <unistd.h>
#include <stdarg.h>
#include <stddef.h>
#include <sys/stat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
fchmod (int fd, mode_t mode)
{
  return MAKE_LEGACY_SYSCALL(FCHMOD_SYSCALL, "syscall|fchmod", (uint64_t) fd, (uint64_t) mode, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
