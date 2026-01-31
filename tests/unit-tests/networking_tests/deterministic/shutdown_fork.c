/* Deterministic shutdown test using socketpair and fork. */
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#define PAYLOAD "shutdown_fork_payload"
#define EXPECTED_BYTES (sizeof(PAYLOAD) - 1)

int main(void)
{
	int sv[2];
	pid_t pid;
	int status;

	assert(socketpair(AF_UNIX, SOCK_STREAM, 0, sv) == 0);

	pid = fork();
	assert(pid >= 0);

	if (pid == 0) {
		close(sv[0]);
		ssize_t n;
		size_t total = 0;
		char buf[64];
		while ((n = read(sv[1], buf, sizeof buf)) > 0)
			total += (size_t)n;
		close(sv[1]);
		if (total != EXPECTED_BYTES)
			exit(1);
		exit(0);
	}

	close(sv[1]);
	assert((size_t)write(sv[0], PAYLOAD, EXPECTED_BYTES) == EXPECTED_BYTES);
	shutdown(sv[0], SHUT_WR);
	close(sv[0]);
	assert(waitpid(pid, &status, 0) == pid);
	assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
	return 0;
}
