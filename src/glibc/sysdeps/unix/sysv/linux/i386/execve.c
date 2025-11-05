#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <alloca.h>

int __execve (const char *__path, char *const __argv[], char *const __envp[])
{
  uint64_t host_path = TRANSLATE_GUEST_POINTER_TO_HOST(__path);
  // Translate argv
  char **guest_argv = (char **)__argv;
  char **guest_envp = (char **)__envp;
  char **host_argv = NULL;
  char **host_envp = NULL;

  if (guest_argv) {
    // Count args
    size_t argc = 0;
    while (TRANSLATE_GUEST_POINTER_TO_HOST(guest_argv[argc]))
      argc++;
    host_argv = alloca((argc + 1) * sizeof(char *));
    for (size_t i = 0; i < argc; i++)
      host_argv[i] = (char *)TRANSLATE_GUEST_POINTER_TO_HOST(guest_argv[i]);
    host_argv[argc] = NULL;
  }

  if (guest_envp) {
    // Count envs
    size_t envc = 0;
    while (TRANSLATE_GUEST_POINTER_TO_HOST(guest_envp[envc]))
      envc++;
    host_envp = alloca((envc + 1) * sizeof(char *));
    for (size_t i = 0; i < envc; i++)
      host_envp[i] = (char *)TRANSLATE_GUEST_POINTER_TO_HOST(guest_envp[i]);
    host_envp[envc] = NULL;
  }

  return MAKE_SYSCALL(EXECVE_SYSCALL, "syscall|execve", host_path, (uint64_t) host_argv, (uint64_t) host_envp, NOTUSED, NOTUSED, NOTUSED)
}
strong_alias (__execve, execve)
