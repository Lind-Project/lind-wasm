#include <unistd.h>
#include <errno.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

/* Make all changes done to FD actually appear on disk.  */
int
fsync (int fd)
{
   return MAKE_LEGACY_SYSCALL(FSYNC_SYSCALL, "syscall|fsync", (uint64_t) fd, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
libc_hidden_def (fsync)
