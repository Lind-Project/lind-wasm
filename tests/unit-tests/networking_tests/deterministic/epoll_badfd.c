#include <sys/epoll.h>
#include <unistd.h>
#include <errno.h>
#include <assert.h>

int main(void) {
    struct epoll_event events[4];

    /* create epoll instance */
    int epfd = epoll_create1(0);
    assert(epfd != -1);

    /* close it so fd becomes invalid */
    assert(close(epfd) == 0);

    /* epoll_wait on closed fd must fail with EBADF */
    int ret = epoll_wait(epfd, events, 4, 1000);
    assert(ret == -1);
    assert(errno == EBADF);

    return 0;
}
