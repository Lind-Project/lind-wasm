#include <sys/xattr.h>
#include <errno.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>
#include <addr_translation.h>

/* List extended attributes associated with the file specified by path.
   If list is NULL and size is zero, returns the buffer size needed.
   Returns the size of the attribute list, or -1 and sets errno on error. */
ssize_t
__listxattr (const char *path, char *list, size_t size)
{
  return MAKE_LEGACY_SYSCALL (LISTXATTR_SYSCALL, "syscall|listxattr",
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (path),
		       (uint64_t) TRANSLATE_GUEST_POINTER_TO_HOST (list),
		       (uint64_t) size,
		       NOTUSED,
		       NOTUSED,
		       NOTUSED, TRANSLATE_ERRNO_ON);
}
weak_alias(__listxattr, listxattr)
