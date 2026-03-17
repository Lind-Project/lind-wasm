#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/epoll.h>

#define MSG "epoll_ready"

int main(void)
{
    int sv[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0)
        return 1;

    int epfd = epoll_create(1);
    if (epfd < 0)
        return 1;

    struct epoll_event ev = { .events = EPOLLIN, .data.fd = sv[1] };
    if (epoll_ctl(epfd, EPOLL_CTL_ADD, sv[1], &ev) != 0)
        return 1;

    size_t len = strlen(MSG);
    if ((size_t)write(sv[0], MSG, len) != len)
        return 1;

    struct epoll_event events[4];
    int n = epoll_wait(epfd, events, 4, 1000);
    if (n != 1 || events[0].data.fd != sv[1] || !(events[0].events & EPOLLIN))
        return 1;

    char buf[32];
    ssize_t r = read(sv[1], buf, sizeof(buf) - 1);
    if (r < 0 || (size_t)r != len || memcmp(buf, MSG, len) != 0)
        return 1;

    close(sv[0]);
    close(sv[1]);
    close(epfd);
    return 0;
}
