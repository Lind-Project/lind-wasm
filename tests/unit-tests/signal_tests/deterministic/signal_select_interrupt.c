// Test: SIGUSR1 interrupts a blocking select on an empty pipe.

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <pthread.h>
#include <sys/select.h>
#include <assert.h>

static volatile int sig_fired = 0;

static void handler(int sig) {
    (void)sig;
    sig_fired = 1;
}

static void *sender(void *arg) {
    pid_t pid = *(pid_t *)arg;
    usleep(500000);
    kill(pid, SIGUSR1);
    return NULL;
}

int main(void) {
    struct sigaction sa;
    sa.sa_handler = handler;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    assert(sigaction(SIGUSR1, &sa, NULL) == 0);

    int pipefd[2];
    assert(pipe(pipefd) == 0);

    pid_t pid = getpid();
    pthread_t t;
    pthread_create(&t, NULL, sender, &pid);

    fd_set rfds;
    FD_ZERO(&rfds);
    FD_SET(pipefd[0], &rfds);
    int ret = select(pipefd[0] + 1, &rfds, NULL, NULL, NULL);

    assert(ret < 0);
    assert(errno == EINTR);
    assert(sig_fired == 1);

    pthread_join(t, NULL);
    close(pipefd[0]);
    close(pipefd[1]);
    printf("signal_select_interrupt: all tests passed\n");
    return 0;
}
