/* Deterministic: child signals parent; parent handler runs. */

#include <assert.h>
#include <signal.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile sig_atomic_t got_int = 0;

static void handler(int sig)
{
	(void)sig;
	got_int = 1;
}

int main(void)
{
	sigset_t block_set, old_mask, empty_set;
	struct sigaction sa = { .sa_handler = handler };
	pid_t parent_pid = getpid();
	pid_t pid;

	sigemptyset(&sa.sa_mask);
	sa.sa_flags = 0;
	assert(sigaction(SIGINT, &sa, NULL) == 0);

	sigemptyset(&block_set);
	sigaddset(&block_set, SIGINT);
	assert(sigprocmask(SIG_BLOCK, &block_set, &old_mask) == 0);

	pid = fork();
	if (pid == 0) {
		if (kill(parent_pid, SIGINT) == 0)
			_exit(0);
		_exit(1);
	}

	assert(got_int == 0);
	sigemptyset(&empty_set);
	assert(sigprocmask(SIG_SETMASK, &empty_set, NULL) == 0);
	while (got_int == 0)
		sigsuspend(&empty_set);

	int status;
	assert(waitpid(pid, &status, 0) == pid);
	assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
	return 0;
}
