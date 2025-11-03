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
  uint64_t host_addr = TRANSLATE_GUEST_POINTER_TO_HOST (addr);
  
  return MAKE_SYSCALL(MPROTECT_SYSCALL, "syscall|mprotect", host_addr, (uint64_t) len, (uint64_t) prot, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias(__GI___mprotect, __mprotect)
strong_alias(__GI___mprotect, mprotect)
