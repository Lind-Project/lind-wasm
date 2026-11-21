/*
 * Test: waitpid() blocked in a non-main thread must be interrupted by
 * exit_group() from the main thread.
 *
 * Scenario:
 *   1. The test process (grandparent) forks a middle process P.
 *   2. P forks a child C that just sleeps, so it is still running when
 *      P goes away. C's pid is sent to the grandparent through a pipe.
 *   3. P's main thread starts a worker thread that blocks in
 *      waitpid(C, ...), then the main thread calls exit(7).
 *   4. exit_group must terminate every thread of P, including the worker
 *      sleeping in waitpid. The grandparent then reaps P with status 7.
 *   5. The grandparent kills the orphaned C so the runtime can shut down.
 *
 * Known failure (see the "waitpid from non-main thread" issue):
 *   waitpid_syscall decides whether to return EINTR by calling
 *   signal_check_trigger(), which only looks at the epoch of the cage's
 *   MAIN thread. epoch_kill_all() marks every thread except the caller,
 *   so when the main thread is the one calling exit_group the main
 *   thread's epoch stays EPOCH_NORMAL. The worker wakes up, sees no
 *   signal, goes back to sleep, and wait_all_threads_exited() in the main
 *   thread never returns. The grandparent's waitpid(P) then hangs and the
 *   harness reports a timeout.
 */

#include <assert.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#define MIDDLE_EXIT_CODE 7

static pid_t grandchild_pid;

static void *waiter(void *arg)
{
    (void)arg;
    int status;
    /* Blocks here: the grandchild only exits after a long sleep. */
    waitpid(grandchild_pid, &status, 0);
    /* Must not be reached: exit_group() from the main thread kills us. */
    _exit(99);
    return NULL;
}

int main(void)
{
    int pipefd[2];
    assert(pipe(pipefd) == 0);

    pid_t middle = fork();
    assert(middle >= 0);

    if (middle == 0) {
        /* ---- middle process P ---- */
        close(pipefd[0]);

        grandchild_pid = fork();
        assert(grandchild_pid >= 0);
        if (grandchild_pid == 0) {
            /* ---- grandchild C: outlive P, bounded so the test cannot hang here ---- */
            close(pipefd[1]);
            sleep(10);
            _exit(0);
        }

        /* Tell the grandparent which pid to clean up. */
        assert(write(pipefd[1], &grandchild_pid, sizeof(grandchild_pid)) ==
               (ssize_t)sizeof(grandchild_pid));
        close(pipefd[1]);

        pthread_t t;
        assert(pthread_create(&t, NULL, waiter, NULL) == 0);

        /* Give the worker time to enter waitpid() and block. */
        sleep(1);

        /* exit_group from the MAIN thread while the worker sleeps in waitpid. */
        exit(MIDDLE_EXIT_CODE);
    }

    /* ---- grandparent ---- */
    close(pipefd[1]);
    pid_t orphan;
    assert(read(pipefd[0], &orphan, sizeof(orphan)) == (ssize_t)sizeof(orphan));
    close(pipefd[0]);

    int status;
    pid_t waited = waitpid(middle, &status, 0); /* hangs when the bug is present */
    assert(waited == middle);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == MIDDLE_EXIT_CODE);

    /* Clean up the orphaned grandchild so the runtime can exit promptly. */
    kill(orphan, SIGKILL);

    printf("Test Passed: exit_group interrupted waitpid in a non-main thread\n");
    return 0;
}
