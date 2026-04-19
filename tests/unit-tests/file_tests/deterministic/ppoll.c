#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <poll.h>
#include <fcntl.h>
#include <string.h>
#include <time.h>
#include <assert.h>

int main(void) {
    int pipefd[2];
    assert(pipe(pipefd) == 0);

    /* Test 1: ppoll with zero timeout returns immediately (no data ready) */
    struct pollfd pfd;
    pfd.fd = pipefd[0];
    pfd.events = POLLIN;
    pfd.revents = 0;

    struct timespec ts_zero = {0, 0};
    int ret = ppoll(&pfd, 1, &ts_zero, NULL);
    assert(ret == 0);

    /* Test 2: ppoll detects data on a readable pipe */
    const char *msg = "hello";
    assert(write(pipefd[1], msg, strlen(msg)) == (ssize_t)strlen(msg));

    pfd.revents = 0;
    struct timespec ts_short = {1, 0};
    ret = ppoll(&pfd, 1, &ts_short, NULL);
    assert(ret == 1);
    assert(pfd.revents & POLLIN);

    /* drain the pipe */
    char buf[16];
    read(pipefd[0], buf, sizeof(buf));

    /* Test 3: ppoll with NULL timeout on a ready fd returns immediately */
    assert(write(pipefd[1], msg, strlen(msg)) == (ssize_t)strlen(msg));
    pfd.revents = 0;
    ret = ppoll(&pfd, 1, NULL, NULL);
    assert(ret == 1);

    close(pipefd[0]);
    close(pipefd[1]);
    printf("ppoll: all tests passed\n");
    return 0;
}
