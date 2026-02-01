/* Deterministic: blocked signals are pending and delivered after unblock. */

#include <assert.h>
#include <signal.h>
#include <unistd.h>

static volatile sig_atomic_t got_usr1 = 0;

static void handler(int sig)
{
	(void)sig;
	got_usr1 = 1;
}

int main(void)
{
	sigset_t block_set, empty_set, pending;
	struct sigaction sa = { .sa_handler = handler };

	sigemptyset(&sa.sa_mask);
	sa.sa_flags = 0;
	assert(sigaction(SIGUSR1, &sa, NULL) == 0);

	sigemptyset(&block_set);
	sigaddset(&block_set, SIGUSR1);
	assert(sigprocmask(SIG_BLOCK, &block_set, NULL) == 0);

	kill(getpid(), SIGUSR1);
	assert(got_usr1 == 0);

	assert(sigpending(&pending) == 0);
	assert(sigismember(&pending, SIGUSR1) != 0);

	sigemptyset(&empty_set);
	assert(sigprocmask(SIG_SETMASK, &empty_set, NULL) == 0);
	while (got_usr1 == 0)
		sigsuspend(&empty_set);

	assert(got_usr1 == 1);
	return 0;
}
