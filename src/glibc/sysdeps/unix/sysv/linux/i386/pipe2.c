#include <unistd.h>
#include <stddef.h>
#include <syscall-template.h>

int
__pipe2 (int pipedes[2], int flags)
{
   return MAKE_SYSCALL(67, "syscall|pipe2", (uint64_t) pipedes, (uint64_t) flags, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

libc_hidden_def (__pipe2)
weak_alias (__pipe2, pipe2)
