#include <unistd.h>
#include <stdint.h>
#include <stddef.h>
#include <syscall-template.h>
#include <stdlib.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>
#include <stdio.h>

#define MAX_EXEC_ARGS 64  // Much smaller - only 512 bytes per array

int __execve (const char *__path, char *const __argv[], char *const __envp[])
{
  uint64_t host_path = TRANSLATE_GUEST_POINTER_TO_HOST(__path);

  // Translate argv
  char **guest_argv = (char **)__argv;
  char **guest_envp = (char **)__envp;
  uint64_t *host_argv = NULL;  // Array of 64-bit host pointers
  uint64_t *host_envp = NULL;  // Array of 64-bit host pointers

  if (guest_argv) {
    // Count args
    size_t argc = 0;
    while (guest_argv[argc] != NULL)
      argc++;

    // Use malloc for heap allocation (avoid stack overflow)
    host_argv = malloc((argc + 1) * sizeof(uint64_t));
    if (!host_argv) {
      errno = ENOMEM;
      return -1;
    }

    // Store full 64-bit host pointers
    for (size_t i = 0; i < argc; i++)
      host_argv[i] = TRANSLATE_GUEST_POINTER_TO_HOST(guest_argv[i]);
    host_argv[argc] = 0;  // NULL terminator
  }

  if (guest_envp) {
    // Count envs
    size_t envc = 0;
    while (guest_envp[envc] != NULL)
      envc++;

    // Use malloc for heap allocation
    host_envp = malloc((envc + 1) * sizeof(uint64_t));
    if (!host_envp) {
      free(host_argv);  // Clean up already allocated memory
      errno = ENOMEM;
      return -1;
    }

    // Store full 64-bit host pointers
    for (size_t i = 0; i < envc; i++)
      host_envp[i] = TRANSLATE_GUEST_POINTER_TO_HOST(guest_envp[i]);
    host_envp[envc] = 0;  // NULL terminator
  }

  // Translate the array pointers themselves
  uint64_t host_argv_ptr = TRANSLATE_GUEST_POINTER_TO_HOST(host_argv);
  uint64_t host_envp_ptr = TRANSLATE_GUEST_POINTER_TO_HOST(host_envp);

  int result = MAKE_SYSCALL(EXECVE_SYSCALL, "syscall|execve",
                            host_path,
                            host_argv_ptr,
                            host_envp_ptr,
                            NOTUSED, NOTUSED, NOTUSED);

  // Clean up heap allocations
  // Note: If execve succeeds, this code won't run (process image replaced)
  // If it fails, we need to clean up
  if (host_argv)
    free(host_argv);
  if (host_envp)
    free(host_envp);

  return result;
}
 
strong_alias (__execve, execve)
libc_hidden_def (__execve)