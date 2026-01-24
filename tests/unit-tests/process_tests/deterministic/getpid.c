#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <sys/types.h>
#include <unistd.h>

int main(void)
{
	pid_t p1 = getpid();
	assert(p1 > 0);

	pid_t p2 = getpid();
	assert(p2 == p1);

	return 0;
}
