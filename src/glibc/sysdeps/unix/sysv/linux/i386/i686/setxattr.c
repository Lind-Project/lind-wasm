#include <sys/xattr.h>
#include <errno.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* Set extended attributes on a file specified by path.
   Returns 0 on success, or -1 and sets errno on error. */
int
__setxattr (const char *path, const char *name, const void *value,
            size_t size, int flags)
{
  return MAKE_LEGACY_SYSCALL (SETXATTR_SYSCALL, "syscall|setxattr",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (path),
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (name),
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (value),
		       (uint64_t) size,
		       (uint64_t) flags,
		       NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias(__setxattr, setxattr)
