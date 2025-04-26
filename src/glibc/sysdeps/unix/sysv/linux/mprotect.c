#include <unistd.h>
#include <sysdep-cancel.h>
#include <syscall-template.h>

int
__GI___mprotect (void *addr, size_t len, int prot)
{
  return MAKE_SYSCALL(177, "syscall|mprotect", (uint64_t) addr, (uint64_t)(uintptr_t) len, (uint64_t) prot, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias(__GI___mprotect, __mprotect)
weak_alias(__GI___mprotect, mprotect)
