#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

__uid_t
__geteuid (void)
{
	return MAKE_LEGACY_SYSCALL(GETEUID_SYSCALL, "syscall|geteuid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, WRAPPED_SYSCALL);
}

weak_alias (__geteuid, geteuid)
