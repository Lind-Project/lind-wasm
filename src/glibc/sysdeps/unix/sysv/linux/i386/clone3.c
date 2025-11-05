#include <stddef.h>  // For size_t
#include <sys/types.h>  // For other system types, if needed
#include <syscall-template.h>
#include <stdlib.h>
#include <string.h>  // For memcpy
#include <lind_syscall_num.h>
#include <addr_translation.h>

int __GI___clone3 (struct clone_args *cl_args, size_t size, int (*func)(void *), void *arg) {
  uint64_t host_cl_args = TRANSLATE_GUEST_POINTER_TO_HOST(cl_args);
  void *local_args = alloca(size);
  memcpy(local_args, (void *)host_cl_args, size);

  uint64_t *child_tid_ptr = (uint64_t *)((char *)local_args + 16);
  if (*child_tid_ptr) {
    *child_tid_ptr = TRANSLATE_GUEST_POINTER_TO_HOST((void *)(*child_tid_ptr));
  }

  int pid = MAKE_SYSCALL(CLONE_SYSCALL, "syscall|clone3", (uint64_t) local_args, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  if(pid == 0 && func != NULL) {
    int ret = func(arg);
    exit(ret);
  }
  return pid;
}
	
weak_alias (__GI___clone3, __clone3)