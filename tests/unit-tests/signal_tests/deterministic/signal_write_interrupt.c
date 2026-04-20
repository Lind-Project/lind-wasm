// Test: SIGALRM interrupts a blocking write on a full pipe.
//
// Expected behavior:
//   1. alarm(1) fires after 1 second, calling alarm_handler.
//   2. The blocking write() on the full pipe returns -1 with errno=EINTR.
//   3. Program prints success and exits 0.
//
// If the write is never interrupted the program hangs indefinitely.

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <assert.h>

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
    assert(sigaction(SIGALRM, &sa, NULL) == 0);

    int pipefd[2];
    assert(pipe(pipefd) == 0);

    // Fill the pipe buffer using O_NONBLOCK so we can detect when it's full.
    int flags = fcntl(pipefd[1], F_GETFL);
    fcntl(pipefd[1], F_SETFL, flags | O_NONBLOCK);
    char buf[4096];
    memset(buf, 0x42, sizeof(buf));
    while (write(pipefd[1], buf, sizeof(buf)) > 0)
        ;
    // Pipe is full. Restore blocking mode.
    fcntl(pipefd[1], F_SETFL, flags);

    alarm(1);

    // This write should block until SIGALRM interrupts it.
    ssize_t ret = write(pipefd[1], buf, 1);

    assert(ret < 0);
    assert(errno == EINTR);
    assert(alarm_fired == 1);

    close(pipefd[0]);
    close(pipefd[1]);
    printf("signal_write_interrupt: all tests passed\n");
    return 0;
}
