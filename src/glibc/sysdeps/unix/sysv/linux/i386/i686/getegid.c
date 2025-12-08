#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

__gid_t
__getegid (void)
{
	return MAKE_SYSCALL(GETEGID_SYSCALL, "syscall|getegid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias (__getegid, getegid)
