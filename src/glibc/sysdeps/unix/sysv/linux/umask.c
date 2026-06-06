#include <unistd.h>
#include <fcntl.h>
#include <string.h>
#include <sys/stat.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

mode_t
umask (mode_t mask)
{
  return (mode_t) MAKE_LEGACY_SYSCALL (
      UMASK_SYSCALL, "syscall|umask", (uint64_t) mask,
      NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}