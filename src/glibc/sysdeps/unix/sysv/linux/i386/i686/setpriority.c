#include <unistd.h>
#include <sys/resource.h>

int
__setpriority (enum __priority_which which, id_t who, int prio)
{
  return -1;
}

weak_alias (__setpriority, setpriority)
