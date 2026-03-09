/*
 * Test: fork() inside a signal handler that interrupted a blocking syscall.
 *
 * Scenario:
 *   1. Create a pipe (read end will block since nothing writes to it)
 *   2. Register a SIGALRM handler that calls fork()
 *   3. Set alarm(1) then call read() on the pipe — read blocks
 *   4. SIGALRM fires, interrupting read() with EINTR
 *   5. Signal handler forks: child exits 42, parent waitpid()s for it
 *   6. After signal handler returns, read() returns -1/EINTR
 *   7. Parent verifies child exited with status 42
 *
 * This exercises the syscall asyncify data path: the EINTR return value
 * must be preserved across the asyncify unwind/rewind cycle that fork
 * triggers inside the signal handler.
 */

#include <assert.h>
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile pid_t child_pid = -1;

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
}

int main(void)
{
    int pipefd[2];
    int ret;

    ret = pipe(pipefd);
    assert(ret == 0);

    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = alarm_handler;
    sa.sa_flags = 0; /* no SA_RESTART — read must return EINTR */
    sigemptyset(&sa.sa_mask);
    ret = sigaction(SIGALRM, &sa, NULL);
    assert(ret == 0);

    /* fire SIGALRM in 1 second */
    alarm(1);

    /* blocking read — nothing will ever be written to this pipe,
     * so it will block until interrupted by SIGALRM */
    char buf[1];
    ret = read(pipefd[0], buf, sizeof(buf));

    /* read should have been interrupted */
    assert(ret == -1);
    assert(errno == EINTR);

    /* signal handler should have forked */
    assert(child_pid > 0);

    /* wait for the child */
    int status;
    pid_t waited = waitpid(child_pid, &status, 0);
    assert(waited == child_pid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 42);

    close(pipefd[0]);
    close(pipefd[1]);

    printf("EINTR fork-in-signal test passed\n");
    fflush(stdout);
    return 0;
}
