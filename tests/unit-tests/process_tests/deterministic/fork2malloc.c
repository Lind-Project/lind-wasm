#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
	// Parent before fork
	char *mptr1 = (char *)malloc(4096);
	assert(mptr1 != NULL);
	memset(mptr1, 0x11, 4096);

	pid_t pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		// Child
		char *mptr2 = (char *)malloc(2048);
		assert(mptr2 != NULL);
		memset(mptr2, 0x22, 2048);

		free(mptr2);
		exit(0);
	} else {
		// Parent
		int status;
		pid_t waited_pid = waitpid(pid, &status, 0);
		assert(waited_pid >= 0);
		assert(WIFEXITED(status));
		assert(WEXITSTATUS(status) == 0);

		// Verify parent's buffer still contains pattern 0x11
		char expected[4096];
		memset(expected, 0x11, 4096);
		assert(memcmp(mptr1, expected, 4096) == 0);

		free(mptr1);
	}

	return 0;
}
