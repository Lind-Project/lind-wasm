#include <unistd.h>

gid_t
__getgid (void)
{
  return -1;
}

gid_t
getgid (void)
{
  return -1;
}
