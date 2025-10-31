#include <unistd.h>
#include <sysdep-cancel.h>
#include <stdint.h>
#include <fcntl.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__GI___mprotect (void *addr, size_t len, int prot)
{
  return MAKE_SYSCALL (MPROTECT_SYSCALL, "syscall|mprotect",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (addr),
		       (uint64_t) len, (uint64_t) prot, NOTUSED, NOTUSED,
		       NOTUSED);
}

weak_alias (__GI___mprotect, __mprotect)
    strong_alias (__GI___mprotect, mprotect)
