#include <unistd.h>
#include <errno.h>
#include <syscall-template.h>

/* Make all changes done to FD actually appear on disk.  */
int
fsync (int fd)
{
   return MAKE_SYSCALL(162, "syscall|fsync", (uint64_t) fd, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
libc_hidden_def (fsync)
