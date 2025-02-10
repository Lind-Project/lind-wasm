#include <unistd.h>

int
__GI_getsid (void)
{
  return -1;
}

weak_alias(__GI_getsid, __setsid)
weak_alias(__GI_getsid, setsid)

