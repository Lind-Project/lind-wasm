/*
 * Test: exit_group — a child thread calls exit() while the main thread waits.
 *
 * Scenario:
 *   1. Parent forks a child
 *   2. Child's main thread creates a worker thread and then blocks on
 *      pthread_join (waiting for the worker to finish)
 *   3. The worker thread calls exit(42) — this should terminate the
 *      entire process (all threads), not just the calling thread
 *   4. Parent waitpid's and verifies exit status 42
 *
 * This exercises exit_group semantics: exit_syscall calls epoch_kill_all
 * to mark other threads for death, then wait_all_threads_exited before
 * doing cage cleanup. The main thread (blocked in pthread_join → futex)
 * should be terminated by the epoch kill mechanism.
 */

#include <assert.h>
#include <pthread.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static void *worker(void *arg)
{
    (void)arg;
    /* Give main thread time to enter pthread_join */
    for (volatile int i = 0; i < 100000; i++)
        ;
    /* This should kill the entire process, not just this thread */
    exit(42);
    return NULL; /* unreachable */
}

int main(void)
{
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        /* child */
        pthread_t t;
        pthread_create(&t, NULL, worker, NULL);
        /* main thread blocks here; worker's exit() should kill us */
        pthread_join(t, NULL);
        /* should not reach here */
        _exit(99);
    }

    /* parent: wait for child */
    int status;
    pid_t waited = waitpid(pid, &status, 0);
    assert(waited == pid);
    assert(WIFEXITED(status));

    assert(WEXITSTATUS(status) == 42);

    return 0;
}
