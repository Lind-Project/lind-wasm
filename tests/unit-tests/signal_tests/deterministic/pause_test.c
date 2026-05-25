/*
 * Test pause(2): blocks until a signal is delivered.
 *
 * Queue SIGUSR1 before calling pause() so the signal is pending
 * immediately.  pause() must return -1/EINTR after the handler runs.
 */

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <unistd.h>

static volatile sig_atomic_t handler_ran = 0;

static void handler(int sig) {
    (void)sig;
    handler_ran = 1;
}

int main(void) {
    struct sigaction sa = {0};
    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    if (sigaction(SIGUSR1, &sa, NULL) != 0) {
        perror("sigaction");
        return 1;
    }

    /* Queue signal before pause() so it is delivered immediately. */
    kill(getpid(), SIGUSR1);

    int ret = pause();

    if (ret != -1 || errno != EINTR) {
        fprintf(stderr, "FAIL: pause returned %d errno %d, expected -1/EINTR\n",
                ret, errno);
        return 1;
    }
    if (!handler_ran) {
        fprintf(stderr, "FAIL: signal handler did not run\n");
        return 1;
    }
    printf("PASS\n");
    return 0;
}
