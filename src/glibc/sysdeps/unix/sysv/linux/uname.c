#include <unistd.h>
#include <sysdep-cancel.h>

int
__GI___uname (int fd, const void *buf, size_t nbytes)
{
  return 0;
}

weak_alias(__GI___uname, __uname)
weak_alias(__GI___uname, __GI_uname)
weak_alias(__GI___uname, uname)
