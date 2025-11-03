#include <unistd.h>

int
__GI___madvise (void *addr, size_t len, int advice)
{
  return 0;
}

weak_alias (__GI___madvise, __madvise)
