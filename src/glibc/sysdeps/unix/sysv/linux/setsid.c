#include <unistd.h>
#include <syscall-template.h>

int
__GI_setsid (void)
{
  return MAKE_SYSCALL(31, "syscall|getpid(setsid)", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias(__GI_setsid, __setsid)
weak_alias(__GI_setsid, setsid)

