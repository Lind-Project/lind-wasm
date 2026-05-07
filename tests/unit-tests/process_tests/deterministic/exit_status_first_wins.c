/*
 * Test: exit_status_first_wins — the first exit() call's code is what
 * the parent sees, even if a competing thread later calls exit(0).
 *
 * Scenario:
 *   1. Parent forks a child.
 *   2. Child spawns a worker thread that calls exit(1).
 *   3. The main thread spins briefly then calls exit(0).
 *   4. Because exit_group_syscall uses a CAS to record the first caller's
 *      status, exit(1) wins and subsequent exit(0) uses the already-
 *      recorded code.
 *   5. Parent's waitpid() should observe exit status 1.
 *
 * Before the fix:
 *   - exit_syscall (thread-only exit) also recorded cage exit status, so
 *     whichever thread ran last would overwrite the earlier code.
 *   - exit_group_syscall did not consult the CAS-recorded status, so
 *     exit(0) could clobber exit(1).
 */

#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static void *worker(void *arg)
{
    (void)arg;
    /* Call exit(1) — this should record exit code 1 via CAS and win. */
    exit(1);
    return NULL; /* unreachable */
}

int main(void)
{
    pid_t pid = fork();
    assert(pid != -1 && "fork should succeed");

    if (pid == 0) {
        pthread_t t;
        int rc = pthread_create(&t, NULL, worker, NULL);
        assert(rc == 0 && "pthread_create should succeed");

        /*
         * Spin long enough for the worker to call exit(1) and have it
         * recorded before we call exit(0).  The epoch-kill from exit(1)
         * will terminate this thread regardless; exit(0) here should
         * either never run or read the already-recorded status (1).
         */
        sleep(1);
        exit(0);
        /* unreachable */
        _exit(99);
    }

    int status;
    pid_t waited = waitpid(pid, &status, 0);
    assert(waited == pid && "waitpid should return child pid");
    assert(WIFEXITED(status) && "child should exit normally");
    assert(WEXITSTATUS(status) == 1 && "first exit(1) should win over later exit(0)");

    printf("exit_status_first_wins: PASS\n");
    return 0;
}
