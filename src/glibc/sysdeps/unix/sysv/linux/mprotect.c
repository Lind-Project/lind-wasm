#include <unistd.h>
#include <sysdep-cancel.h>

int
__GI___mprotect (int fd, const void *buf, size_t nbytes)
{

  return 0;
}

weak_alias(__GI___mprotect, __mprotect)