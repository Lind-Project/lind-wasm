/*
 * Non-blocking I/O tests: EAGAIN on empty recv, O_NONBLOCK via fcntl,
 * SOCK_NONBLOCK flag, non-blocking accept.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <netinet/in.h>

#define PORT 49220

int main(void) {
    /* --- 1) SOCK_NONBLOCK at socket creation --- */
    int s = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
    assert(s >= 0);

    int flags = fcntl(s, F_GETFL);
    assert(flags >= 0);
    assert(flags & O_NONBLOCK);
    printf("1. SOCK_NONBLOCK flag set at creation\n");
    close(s);

    /* --- 2) Set O_NONBLOCK via fcntl --- */
    s = socket(AF_INET, SOCK_STREAM, 0);
    assert(s >= 0);

    flags = fcntl(s, F_GETFL);
    assert(!(flags & O_NONBLOCK));

    assert(fcntl(s, F_SETFL, flags | O_NONBLOCK) == 0);
    flags = fcntl(s, F_GETFL);
    assert(flags & O_NONBLOCK);
    printf("2. O_NONBLOCK set via fcntl\n");
    close(s);

    /* --- 3) EAGAIN on non-blocking recv from empty socketpair --- */
    int pair[2];
    assert(socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, pair) == 0);

    char buf[64];
    errno = 0;
    ssize_t n = recv(pair[0], buf, sizeof(buf), 0);
    assert(n == -1);
    assert(errno == EAGAIN || errno == EWOULDBLOCK);
    printf("3. recv on empty non-blocking socket → EAGAIN\n");

    close(pair[0]);
    close(pair[1]);

    /* --- 4) Non-blocking accept (no pending connection → EAGAIN) --- */
    int srv = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
    assert(srv >= 0);

    int yes = 1;
    assert(setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes)) == 0);

    struct sockaddr_in addr = {0};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(PORT);
    assert(bind(srv, (struct sockaddr *)&addr, sizeof(addr)) == 0);
    assert(listen(srv, 1) == 0);

    errno = 0;
    int c = accept(srv, NULL, NULL);
    assert(c == -1);
    assert(errno == EAGAIN || errno == EWOULDBLOCK);
    printf("4. Non-blocking accept with no client → EAGAIN\n");

    /* --- 5) SOCK_NONBLOCK | SOCK_CLOEXEC combo --- */
    int combo = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
    assert(combo >= 0);

    flags = fcntl(combo, F_GETFL);
    assert(flags & O_NONBLOCK);

    int fdflags = fcntl(combo, F_GETFD);
    assert(fdflags & FD_CLOEXEC);
    printf("5. SOCK_NONBLOCK | SOCK_CLOEXEC both set\n");
    close(combo);

    /* --- 6) Non-blocking send filling buffer --- */
    assert(socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, pair) == 0);

    /* Shrink send buffer to make it fill faster */
    int small = 4096;
    setsockopt(pair[0], SOL_SOCKET, SO_SNDBUF, &small, sizeof(small));

    char bigbuf[65536];
    memset(bigbuf, 'A', sizeof(bigbuf));

    ssize_t total = 0;
    int eagain_hit = 0;
    for (int i = 0; i < 100; i++) {
        n = send(pair[0], bigbuf, sizeof(bigbuf), MSG_DONTWAIT);
        if (n == -1) {
            assert(errno == EAGAIN || errno == EWOULDBLOCK);
            eagain_hit = 1;
            break;
        }
        total += n;
    }
    assert(eagain_hit);
    assert(total > 0);
    printf("6. Non-blocking send filled buffer (%zd bytes), then EAGAIN\n", total);

    close(pair[0]);
    close(pair[1]);
    close(srv);

    printf("All non-blocking I/O tests passed\n");
    return 0;
}
