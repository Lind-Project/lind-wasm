#include <unistd.h>
#include <fcntl.h>
#include <string.h>

ssize_t __GI_readlinkat (int __fd, const char *__file_name, char *__buf, size_t __len)
{
  return 0;
}

weak_alias(__GI_readlinkat, _readlinkat)
