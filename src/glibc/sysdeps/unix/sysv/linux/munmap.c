#include <unistd.h>
#include <sysdep-cancel.h>
#include <stdint.h>
#include <fcntl.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__GI___munmap (void *addr, size_t len)
{
  // return MAKE_SYSCALL(22, "syscall|munmap", (uint64_t)(uintptr_t) addr, (uint64_t) len, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  return MAKE_SYSCALL(MUNMAP_SYSCALL, "syscall|munmap", (uint64_t)(uintptr_t) addr, (uint64_t) len, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  // return 0;
}

int munmap (void *addr, size_t len)
{
  // return MAKE_SYSCALL(22, "syscall|munmap", (uint64_t)(uintptr_t) addr, (uint64_t) len, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  return MAKE_SYSCALL(MUNMAP_SYSCALL, "syscall|munmap", (uint64_t)(uintptr_t) addr, (uint64_t) len, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
  // return 0;
}

weak_alias(__GI___munmap, __munmap)
