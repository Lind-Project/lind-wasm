#include <stddef.h>  // For size_t
#include <sys/types.h>  // For other system types, if needed
#include <syscall-template.h>
#include <stdlib.h>
#include <string.h>  // For memcpy
#include <lind_syscall_num.h>
#include <addr_translation.h>

// Minimum offset for child_tid_ptr (16), assumes clone_args is at least this long
#define CHILD_TID_OFFSET 16
#define CHILD_TID_SIZE sizeof(uint64_t)
#define MIN_CLONE_ARGS_SIZE (CHILD_TID_OFFSET + CHILD_TID_SIZE)

int __GI___clone3 (struct clone_args *cl_args, size_t size, int (*func)(void *), void *arg) {  
  void *local_args = alloca(size);
  if (!local_args) {
      errno = ENOMEM;
      return -1;
  }

  // Copy from guest address - glibc can access guest memory directly
  memcpy(local_args, (void *)cl_args, size);

  uint64_t *child_tid_ptr = (uint64_t *)((char *)local_args + CHILD_TID_OFFSET);
  if (child_tid_ptr) {
      uint64_t child_tid_value = *child_tid_ptr;
      if (child_tid_value) {
          uint64_t translated = TRANSLATE_GUEST_POINTER_TO_HOST((void *)child_tid_value);
          if (translated == 0) {
              *child_tid_ptr = 0;
          } else {
              *child_tid_ptr = translated;
          }
      }
  }

  // Translate the local_args pointer to host address for syscall
  uint64_t host_local_args = TRANSLATE_GUEST_POINTER_TO_HOST(local_args);
  if (host_local_args == 0) {
      errno = EFAULT;
      return -1;
  }

  int pid = MAKE_SYSCALL(CLONE_SYSCALL, "syscall|clone3", host_local_args, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  
  if (pid == 0 && func != NULL) {
      int ret = func(arg);
      exit(ret);
  }
  
  return pid;
}

weak_alias (__GI___clone3, __clone3)