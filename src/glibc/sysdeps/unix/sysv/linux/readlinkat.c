#include <unistd.h>
#include <fcntl.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>

/* Read the contents of the symbolic link PATH into no more than
   LEN bytes of BUF.  The contents are not null-terminated.
   Returns the number of characters read, or -1 for errors.  */
/*
* Edit Note:
* We implement both `readlink` and `readlinkat` in RawPOSIX, so changed the normal 
*/
ssize_t
__libc_readlinkat (int fd, const char *path, char *buf, size_t len)
{
  return MAKE_SYSCALL(166, "syscall|readlinkat",(uint64_t) fd, (uint64_t) path, (uint64_t)(uintptr_t) buf, (uint64_t) len, NOTUSED, NOTUSED);
}
weak_alias(__libc_readlinkat, readlinkat)
