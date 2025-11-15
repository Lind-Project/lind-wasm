#include <stddef.h>  // For size_t
#include <sys/types.h>  // For other system types, if needed
#include <syscall-template.h>
#include <stdlib.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>
#include <clone3.h>

int __GI___clone3 (struct clone_args *cl_args, size_t size, int (*func)(void *), void *arg) {
  uint64_t guest_child_tid = cl_args->child_tid;
  cl_args->child_tid = TRANSLATE_GUEST_POINTER_TO_HOST(guest_child_tid);

  uint64_t host_cl_args = TRANSLATE_GUEST_POINTER_TO_HOST(cl_args);
  int pid = MAKE_SYSCALL(CLONE_SYSCALL, "syscall|clone3",
                         host_cl_args,
                         NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);

  // Execute child function if in child process
  if (pid == 0 && func != NULL) {
    int ret = func(arg);
    exit(ret);
  }

  return pid;
}

weak_alias (__GI___clone3, __clone3)
