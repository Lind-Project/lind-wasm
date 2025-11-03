#include <unistd.h>

int
__setpgid (int pid, int pgid)
{
  return -1;
}

int
setpgid (int pid, int pgid)
{
  return -1;
}
