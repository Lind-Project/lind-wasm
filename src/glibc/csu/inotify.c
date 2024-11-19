#include <stdint.h>

// BUG: add these two function because coreutils needs them but
// it is not found in glibc. (And we do not support this feature right now)
// - Qianxi Chen
int inotify_add_watch (int __fd, const char *__name, uint32_t __mask)
{
    return 0;
}

int inotify_rm_watch (int __fd, int __wd)
{
    return 0;
}
