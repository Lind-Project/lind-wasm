#include <unistd.h>
#include <stddef.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__pipe2 (int pipedes[2], int flags)
{
  return MAKE_SYSCALL (PIPE2_SYSCALL, "syscall|pipe2",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (pipedes),
		       (uint64_t) flags, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

libc_hidden_def (__pipe2) weak_alias (__pipe2, pipe2)
