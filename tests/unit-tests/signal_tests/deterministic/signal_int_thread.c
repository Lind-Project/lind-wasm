#include <signal.h>
#include <unistd.h>
#include <pthread.h>

#define N 2

static volatile sig_atomic_t got_int = 0;

static void handler(int sig)
{
	(void)sig;
	got_int = 1;
}

static void *thread_func(void *arg)
{
	pthread_barrier_t *barrier = (pthread_barrier_t *)arg;
	pthread_barrier_wait(barrier);
	return NULL;
}

int main(void)
{
	sigset_t block_set, empty_set;
	struct sigaction sa = { .sa_handler = handler };
	pthread_barrier_t barrier;
	pthread_t t[N];

	sigemptyset(&sa.sa_mask);
	sa.sa_flags = 0;
	sigaction(SIGINT, &sa, NULL);

	sigemptyset(&block_set);
	sigaddset(&block_set, SIGINT);
	pthread_sigmask(SIG_BLOCK, &block_set, NULL);

	sigemptyset(&empty_set);
	pthread_barrier_init(&barrier, NULL, N);

	for (int i = 0; i < N; i++)
		pthread_create(&t[i], NULL, thread_func, &barrier);

	kill(getpid(), SIGINT);
	if (got_int != 0)
		return 1;

	pthread_sigmask(SIG_UNBLOCK, &block_set, NULL);
	while (got_int == 0)
		sigsuspend(&empty_set);

	for (int i = 0; i < N; i++)
		pthread_join(t[i], NULL);

	return 0;
}
