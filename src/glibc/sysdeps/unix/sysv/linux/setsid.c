#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__GI_setsid (void)
{
  /* In lind-wasm, session ID (sid) and process ID (pid) are the same,
     so setsid returns the process ID. */
  return MAKE_TRADITION(GETPID_SYSCALL, "syscall|getpid(setsid)", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

weak_alias(__GI_setsid, __setsid)
weak_alias(__GI_setsid, setsid)

