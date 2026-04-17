// Test: SIGALRM interrupts a blocking write on a full pipe.
//
// Expected behavior:
//   1. alarm(1) fires after 1 second, calling alarm_handler.
//   2. The blocking write() on the full pipe returns -1 with errno=EINTR.
//   3. Program prints "PASS" and exits 0.
//
// If the write is never interrupted the program hangs indefinitely.

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile int alarm_fired = 0;

static void alarm_handler(int sig) {
    (void)sig;
    alarm_fired = 1;
}

int main(void) {
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGALRM, &sa, NULL);

    int pipefd[2];
    if (pipe(pipefd) < 0) {
        perror("pipe");
        return 1;
    }

    // Fill the pipe buffer using O_NONBLOCK so we can detect when it's full.
    int flags = fcntl(pipefd[1], F_GETFL);
    fcntl(pipefd[1], F_SETFL, flags | O_NONBLOCK);
    char buf[4096];
    memset(buf, 0x42, sizeof(buf));
    while (write(pipefd[1], buf, sizeof(buf)) > 0)
        ;
    // Pipe is full (last write returned EAGAIN). Restore blocking mode.
    fcntl(pipefd[1], F_SETFL, flags);

    alarm(1);

    // This write should block until SIGALRM interrupts it.
    ssize_t ret = write(pipefd[1], buf, 1);

    if (ret < 0 && errno == EINTR && alarm_fired) {
        printf("PASS\n");
        close(pipefd[0]);
        close(pipefd[1]);
        return 0;
    }

    // If we reach here without alarm_fired, the write returned for
    // another reason (or the pipe wasn't full).
    if (!alarm_fired) {
        printf("FAIL: write returned without SIGALRM (ret=%zd errno=%d)\n",
               ret, errno);
    } else {
        printf("FAIL: write returned with alarm_fired but errno=%d (expected EINTR=%d)\n",
               errno, EINTR);
    }
    close(pipefd[0]);
    close(pipefd[1]);
    return 1;
}
