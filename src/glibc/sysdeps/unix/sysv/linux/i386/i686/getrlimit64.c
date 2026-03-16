#include <sys/resource.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int
__getrlimit64 (int resource, struct rlimit64 *rlimits)
{
    return MAKE_LEGACY_SYSCALL(PRLIMIT64_SYSCALL, "syscall|prlimit64",
        0, (uint64_t) resource,
        0, (uint64_t) rlimits,
        NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}
libc_hidden_def (__getrlimit64)
weak_alias (__getrlimit64, getrlimit64)
weak_alias (__getrlimit64, getrlimit)
weak_alias (__getrlimit64, __getrlimit)