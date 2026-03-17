#include <assert.h>
#include <unistd.h>

int main(void)
{
	int pipefd[2];
	assert(pipe(pipefd) == 0);

	int saved = dup(STDOUT_FILENO);
	assert(saved >= 0);

	assert(dup2(pipefd[1], STDOUT_FILENO) == STDOUT_FILENO);
	close(pipefd[1]);
	pipefd[1] = -1;

	int dupfd = dup(STDOUT_FILENO);
	assert(dupfd >= 0 && dupfd != STDOUT_FILENO);
	assert(write(STDOUT_FILENO, "A", 1) == 1);
	assert(write(dupfd, "B", 1) == 1);
	close(dupfd);

	assert(dup2(saved, STDOUT_FILENO) == STDOUT_FILENO);
	close(saved);

	char buf[4];
	ssize_t n = read(pipefd[0], buf, 2);
	assert(n == 2 && buf[0] == 'A' && buf[1] == 'B');
	n = read(pipefd[0], buf, 1);
	assert(n == 0);
	close(pipefd[0]);

	return 0;
}
