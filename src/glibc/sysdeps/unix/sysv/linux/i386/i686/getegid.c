#include <unistd.h>
#include <syscall-template.h>

__gid_t
__getegid (void)
{
	return MAKE_SYSCALL(53, "syscall|getegid", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED);
}

weak_alias (__getegid, getegid)
