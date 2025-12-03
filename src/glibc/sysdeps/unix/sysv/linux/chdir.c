#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__chdir (const char *__path)
{
  uint64_t host_path = TRANSLATE_GUEST_POINTER_TO_HOST (__path);
  
  return MAKE_LEGACY_SYSCALL (CHDIR_SYSCALL, "syscall|chdir",
		       host_path, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

int
chdir (const char *__path)
{
  uint64_t host_path = TRANSLATE_GUEST_POINTER_TO_HOST (__path);
  
  return MAKE_LEGACY_SYSCALL (CHDIR_SYSCALL, "syscall|chdir",
		       host_path, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
