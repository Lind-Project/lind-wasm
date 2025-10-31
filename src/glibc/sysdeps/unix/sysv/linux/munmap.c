#include <unistd.h>
#include <sysdep-cancel.h>
#include <stdint.h>
#include <fcntl.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
__GI___munmap (void *addr, size_t len)
{
  return MAKE_SYSCALL (MUNMAP_SYSCALL, "syscall|munmap",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (addr),
		       (uint64_t) len, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias (__GI___munmap, __munmap) weak_alias (__GI___munmap, munmap)
