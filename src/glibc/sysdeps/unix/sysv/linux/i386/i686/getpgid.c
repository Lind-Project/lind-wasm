#include <unistd.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

pid_t
__getpgid (pid_t pid)
{
  return MAKE_LEGACY_SYSCALL (GETPGID_SYSCALL, "syscall|getpgid",
               (uint64_t) pid, NOTUSED, NOTUSED,
               NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias (__getpgid, getpgid)
