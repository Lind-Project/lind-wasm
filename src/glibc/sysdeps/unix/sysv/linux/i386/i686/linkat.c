#include <unistd.h>
#include <fcntl.h>
#include <string.h>

#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

int
linkat (int fromfd, const char *from, int tofd, const char *to, int at_flags)
{
  uint64_t host_from = TRANSLATE_GUEST_POINTER_TO_HOST (from);
  uint64_t host_to = TRANSLATE_GUEST_POINTER_TO_HOST (to);

  return MAKE_LEGACY_SYSCALL (LINKAT_SYSCALL, "syscall|linkat",
      (uint64_t) fromfd, host_from, (uint64_t) tofd, host_to,
      (uint64_t) at_flags, NOTUSED, TRANSLATE_ERRNO_ON);
}
