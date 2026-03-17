#include <signal.h>
#include <unistd.h>

static volatile sig_atomic_t got_usr1 = 0;
static volatile sig_atomic_t got_usr2 = 0;
static volatile sig_atomic_t got_int = 0;

static void handler_usr1(int sig)
{
	(void)sig;
	got_usr1 = 1;
}

static void handler_usr2(int sig)
{
	(void)sig;
	got_usr2 = 1;
}

static void handler_int(int sig)
{
	(void)sig;
	got_int = 1;
}

int main(void)
{
	sigset_t block_set, empty_set;
	struct sigaction sa = { .sa_handler = handler_usr1, .sa_flags = 0 };

	sigemptyset(&sa.sa_mask);
	sigaction(SIGUSR1, &sa, NULL);

	sa.sa_handler = handler_usr2;
	sigaction(SIGUSR2, &sa, NULL);

	sa.sa_handler = handler_int;
	sigaction(SIGINT, &sa, NULL);

	sigemptyset(&block_set);
	sigaddset(&block_set, SIGUSR1);
	sigaddset(&block_set, SIGUSR2);
	sigaddset(&block_set, SIGINT);
	sigprocmask(SIG_BLOCK, &block_set, NULL);

	kill(getpid(), SIGUSR1);
	kill(getpid(), SIGUSR2);
	kill(getpid(), SIGINT);

	if (got_usr1 != 0 || got_usr2 != 0 || got_int != 0)
		return 1;

	sigemptyset(&empty_set);
	sigprocmask(SIG_UNBLOCK, &block_set, NULL);

	while (got_usr1 == 0 || got_usr2 == 0 || got_int == 0)
		sigsuspend(&empty_set);

	if (got_usr1 != 1 || got_usr2 != 1 || got_int != 1)
		return 1;

	return 0;
}
