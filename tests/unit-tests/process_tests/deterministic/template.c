#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#define PID_ANY (-1)

int main(void)
{
	int pret = -1, cret = -1, ppret = -1;
	pid_t pid = -1, cpid = -1, ppid = -1;

	pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		// Child
		cpid = fork();
		assert(cpid >= 0);

		if (cpid == 0) {
			// Grandchild
			exit(EXIT_SUCCESS);
		}

		// Child waits for any child
		pid_t waited = waitpid(PID_ANY, &cret, 0);
		assert(waited >= 0);
		assert(WIFEXITED(cret));
		assert(WEXITSTATUS(cret) == 0);

		exit(EXIT_SUCCESS);
	}

	// Parent waits for first child
	pid_t waited1 = waitpid(pid, &pret, 0);
	assert(waited1 >= 0);
	assert(WIFEXITED(pret));
	assert(WEXITSTATUS(pret) == 0);

	// Parent forks again
	ppid = fork();
	assert(ppid >= 0);

	if (ppid == 0) {
		// Second child
		exit(EXIT_SUCCESS);
	}

	// Parent waits for second child
	pid_t waited2 = wait(&ppret);
	assert(waited2 >= 0);
	assert(WIFEXITED(ppret));
	assert(WEXITSTATUS(ppret) == 0);

	return 0;
}
