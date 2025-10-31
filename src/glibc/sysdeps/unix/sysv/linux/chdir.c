#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__chdir (const char *__path)
{
  return MAKE_SYSCALL (CHDIR_SYSCALL, "syscall|chdir",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (__path),
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

int
chdir (const char *__path)
{
  return MAKE_SYSCALL (CHDIR_SYSCALL, "syscall|chdir",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (__path),
		       NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}
