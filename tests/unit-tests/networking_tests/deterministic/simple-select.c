#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
	int fd[2];
	int ret = pipe(fd);
	assert(ret == 0);

	pid_t pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		// Child
		close(fd[1]);

		fd_set readfds;
		FD_ZERO(&readfds);
		FD_SET(fd[0], &readfds);

		int select_ret = select(fd[0] + 1, &readfds, NULL, NULL, NULL);
		assert(select_ret == 1);
		assert(FD_ISSET(fd[0], &readfds));

		const char *expected = "PING";
		size_t len = strlen(expected);
		char buf[128];
		size_t total_read = 0;

		while (total_read < len) {
			ssize_t n = read(fd[0], buf + total_read, len - total_read);
			assert(n > 0);
			total_read += n;
		}

		assert(memcmp(buf, expected, len) == 0);

		close(fd[0]);
		exit(0);
	} else {
		// Parent
		close(fd[0]);

		const char *msg = "PING";
		size_t len = strlen(msg);
		size_t total_written = 0;

		while (total_written < len) {
			ssize_t n = write(fd[1], msg + total_written, len - total_written);
			assert(n > 0);
			total_written += n;
		}

		close(fd[1]);

		int status;
		pid_t waited_pid = waitpid(pid, &status, 0);
		assert(waited_pid >= 0);
		assert(WIFEXITED(status));
		assert(WEXITSTATUS(status) == 0);
	}

	return 0;
}
