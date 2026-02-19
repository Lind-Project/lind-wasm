#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
	pid_t pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		// Child
		char *ptr = (char *)malloc(1024);
		assert(ptr != NULL);

		memset(ptr, 0xAB, 1024);

		free(ptr);
		exit(0);
	} else {
		// Parent
		int status;
		pid_t waited_pid = waitpid(pid, &status, 0);
		assert(waited_pid >= 0);
		assert(WIFEXITED(status));
		assert(WEXITSTATUS(status) == 0);
	}

	return 0;
}
