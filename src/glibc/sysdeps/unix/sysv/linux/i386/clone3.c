#include <stddef.h>    // For size_t
#include <sys/types.h> // For other system types, if needed
#include <syscall-template.h>
#include <stdlib.h>
#include <lind_syscall_num.h>

int
__GI___clone3 (struct clone_args *cl_args, size_t size, int (*func) (void *),
	       void *arg)
{
  int pid = MAKE_SYSCALL (CLONE_SYSCALL, "syscall|clone3", (uint64_t) cl_args,
			  NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  if (pid == 0 && func != NULL)
    {
      int ret = func (arg);
      exit (ret);
    }
  return pid;
}

weak_alias (__GI___clone3, __clone3)
