#include <stdio.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>
#include <stdlib.h>

static volatile sig_atomic_t got_signal = 0;

void handler(int sig) {
    got_signal = 1;
}

int main(void) {
    struct sigaction sa;

    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;

    if (sigaction(SIGALRM, &sa, NULL) == -1) {
        perror("sigaction");
        return 1;
    }

    alarm(1);

    printf("Waiting in pause()...\n");

    int ret = pause();

    if (ret == -1 && errno == EINTR && got_signal) {
        printf("pause() was interrupted by SIGALRM: test passed\n");
        return 0;
    }

    printf("test failed: pause() returned %d, errno=%d, got_signal=%d\n",
           ret, errno, got_signal);

    return 1;
}