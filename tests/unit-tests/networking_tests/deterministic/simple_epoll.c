#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/epoll.h>
#include <string.h>
#include <errno.h>

int main(void) {
    int fds[2];
    if (pipe(fds) < 0) {
        perror("pipe");
        exit(1);
    }

    int epfd = epoll_create(1);
    if (epfd < 0) {
        perror("epoll_create");
        exit(1);
    }

    struct epoll_event ev, events[1];
    ev.events = EPOLLIN;
    ev.data.fd = fds[0];  

    if (epoll_ctl(epfd, EPOLL_CTL_ADD, fds[0], &ev) < 0) {
        perror("epoll_ctl");
        exit(1);
    }

    const char *msg = "hello epoll!\n";
    if (write(fds[1], msg, strlen(msg)) < 0) {
        perror("write");
        exit(1);
    }

    printf("waiting for epoll event...\n");

    int n = epoll_wait(epfd, events, 1, 10000); // Wait 1s at most
    if (n < 0) {
        perror("epoll_wait");
        exit(1);
    } else if (n == 0) {
        printf("timeout, no events\n");
        exit(EXIT_FAILURE);
    } else {
        if (events[0].events & EPOLLIN) {
            char buf[128];
            int r = read(events[0].data.fd, buf, sizeof(buf) - 1);
            if (r > 0) {
                buf[r] = '\0';
                printf("got data: %s\n", buf);
            }
        }
    }

    close(fds[0]);
    close(fds[1]);
    close(epfd);
    return 0;
}
