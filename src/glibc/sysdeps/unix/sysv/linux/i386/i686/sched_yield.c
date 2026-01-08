#include <unistd.h>
#include <syscall-template.h>
#include <lind_syscall_num.h>

int __GI___sched_yield (void)
{
	return MAKE_LEGACY_SYSCALL(SCHED_YIELD_SYSCALL, "syscall|sched_yield", NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, NOTUSED, TRANSLATE_ERRNO_ON);
}

weak_alias(__GI___sched_yield, __sched_yield)
weak_alias(__GI___sched_yield, sched_yield)
