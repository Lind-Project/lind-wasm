/*
 * Test: fork() inside a signal handler (via epoch-delivered SIGALRM).
 *
 * Scenario:
 *   1. Register a SIGALRM handler that calls fork()
 *   2. Set alarm(1) then spin-wait (wasm execution allows epoch to fire)
 *   3. SIGALRM fires via epoch callback, signal handler runs
 *   4. Signal handler forks: child exits 42, parent saves child pid
 *   5. Parent verifies child exited with status 42
 *
 * This exercises the asyncify unwind/rewind cycle that fork triggers
 * inside a signal handler. The signal_asyncify_data must be preserved
 * correctly for the child to start and the parent to resume.
 *
 * NOTE: Testing the EINTR-specific code path (fork inside a signal
 * handler that interrupted a *blocking* syscall) requires RawPOSIX to
 * support interrupting host blocking calls, which is not yet implemented.
 */

#include <assert.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile pid_t child_pid = -1;
static volatile sig_atomic_t handler_ran = 0;

static void alarm_handler(int sig)
{
    (void)sig;
    pid_t pid = fork();
    if (pid == 0) {
        /* child: exit with a recognizable status */
        _exit(42);
    }
    /* parent: save child pid for later waitpid */
    child_pid = pid;
    handler_ran = 1;
}

int main(void)
{
    int ret;

    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = alarm_handler;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    ret = sigaction(SIGALRM, &sa, NULL);
    assert(ret == 0);

    /* fire SIGALRM in 1 second */
    alarm(1);

    /* spin until handler fires — epoch will deliver SIGALRM
     * during wasm execution */
    while (!handler_ran)
        ;

    /* signal handler should have forked */
    assert(child_pid > 0);

    /* wait for the child */
    int status;
    pid_t waited = waitpid(child_pid, &status, 0);
    assert(waited == child_pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 42);

    printf("fork-in-signal-handler test passed\n");
    fflush(stdout);
    return 0;
}
