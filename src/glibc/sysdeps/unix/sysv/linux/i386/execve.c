#include <unistd.h>
#include <stdint.h>
#include <stddef.h>
#include <syscall-template.h>
#include <stdlib.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int __execve (const char *__path, char *const __argv[], char *const __envp[])
{
  uint64_t host_path = TRANSLATE_GUEST_POINTER_TO_HOST(__path);

  size_t argc = 0;
  if (__argv) 
  {
    while (__argv[argc] != NULL)
      argc++;
  }

  size_t envc = 0;
  if (__envp) 
  {
    while (__envp[envc] != NULL)
      envc++;
  }

  uint64_t host_argv[argc+1];
  uint64_t host_envp[envc+1];

  if (__argv) 
  {
    for (size_t i = 0; i < argc; i++)
      host_argv[i] = TRANSLATE_GUEST_POINTER_TO_HOST(__argv[i]);
    host_argv[argc] = 0;
  }

  if (__envp) 
  {
    for (size_t i = 0; i < envc; i++)
      host_envp[i] = TRANSLATE_GUEST_POINTER_TO_HOST(__envp[i]);
    host_envp[envc] = 0;
  }

  uint64_t host_argv_ptr = TRANSLATE_GUEST_POINTER_TO_HOST(host_argv);
  uint64_t host_envp_ptr = TRANSLATE_GUEST_POINTER_TO_HOST(host_envp);

  return MAKE_TRANDITION(EXECVE_SYSCALL, "syscall|execve", host_path, host_argv_ptr, host_envp_ptr, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}
strong_alias (__execve, execve)
libc_hidden_def (__execve)
