/*
 * Networking error handling: EBADF, ENOTCONN, ECONNREFUSED, EPIPE,
 * EINVAL, EADDRINUSE, double-close safety.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <errno.h>
#include <signal.h>
#include <sys/socket.h>
#include <netinet/in.h>

#define PORT_BASE 49230

int main(void) {
    /* Ignore SIGPIPE so writes to broken pipes return EPIPE instead of killing us */
    signal(SIGPIPE, SIG_IGN);

    /* --- 1) Operations on bad FD → EBADF --- */
    char buf[16];

    errno = 0;
    assert(recv(-1, buf, sizeof(buf), 0) == -1);
    assert(errno == EBADF);

    errno = 0;
    assert(send(-1, "x", 1, 0) == -1);
    assert(errno == EBADF);

    errno = 0;
    assert(accept(-1, NULL, NULL) == -1);
    assert(errno == EBADF);

    errno = 0;
    struct sockaddr_in dummy = {0};
    dummy.sin_family = AF_INET;
    assert(bind(-1, (struct sockaddr *)&dummy, sizeof(dummy)) == -1);
    assert(errno == EBADF);

    errno = 0;
    assert(listen(-1, 1) == -1);
    assert(errno == EBADF);

    errno = 0;
    assert(connect(-1, (struct sockaddr *)&dummy, sizeof(dummy)) == -1);
    assert(errno == EBADF);

    errno = 0;
    assert(shutdown(-1, SHUT_RDWR) == -1);
    assert(errno == EBADF);

    printf("1. EBADF on all operations with fd=-1\n");

    /* --- 2) recv on unconnected TCP socket → ENOTCONN --- */
    int s = socket(AF_INET, SOCK_STREAM, 0);
    assert(s >= 0);

    errno = 0;
    assert(recv(s, buf, sizeof(buf), 0) == -1);
    assert(errno == ENOTCONN);
    printf("2. recv on unconnected socket → ENOTCONN\n");
    close(s);

    /* --- 3) ECONNREFUSED: connect to port with nobody listening --- */
    s = socket(AF_INET, SOCK_STREAM, 0);
    assert(s >= 0);

    struct sockaddr_in refuse = {0};
    refuse.sin_family = AF_INET;
    refuse.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    refuse.sin_port = htons(PORT_BASE); /* nobody listening here */

    errno = 0;
    int ret = connect(s, (struct sockaddr *)&refuse, sizeof(refuse));
    assert(ret == -1);
    assert(errno == ECONNREFUSED);
    printf("3. connect to closed port → ECONNREFUSED\n");
    close(s);

    /* --- 4) EPIPE: write after peer shutdown --- */
    int pair[2];
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);

    close(pair[1]); /* close the reader */

    /* First write may succeed (kernel buffer). Keep writing until EPIPE. */
    int got_epipe = 0;
    for (int i = 0; i < 100; i++) {
        errno = 0;
        ssize_t n = send(pair[0], "data", 4, MSG_NOSIGNAL);
        if (n == -1 && errno == EPIPE) {
            got_epipe = 1;
            break;
        }
    }
    assert(got_epipe);
    printf("4. send after peer close → EPIPE\n");
    close(pair[0]);

    /* --- 5) EADDRINUSE: bind same port twice --- */
    int s1 = socket(AF_INET, SOCK_STREAM, 0);
    int s2 = socket(AF_INET, SOCK_STREAM, 0);
    assert(s1 >= 0 && s2 >= 0);

    struct sockaddr_in addr = {0};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = htons(PORT_BASE + 1);

    assert(bind(s1, (struct sockaddr *)&addr, sizeof(addr)) == 0);
    assert(listen(s1, 1) == 0);

    errno = 0;
    ret = bind(s2, (struct sockaddr *)&addr, sizeof(addr));
    assert(ret == -1);
    assert(errno == EADDRINUSE);
    printf("5. bind same port twice → EADDRINUSE\n");

    close(s1);
    close(s2);

    /* --- 6) EINVAL: listen on unbound socket --- */
    s = socket(AF_INET, SOCK_STREAM, 0);
    assert(s >= 0);

    /* On Linux, listen on unbound socket actually succeeds (auto-binds).
     * But listen with negative backlog is EINVAL on some systems.
     * Test shutdown with invalid 'how' instead. */
    errno = 0;
    ret = shutdown(s, 99); /* invalid 'how' parameter */
    assert(ret == -1);
    assert(errno == EINVAL);
    printf("6. shutdown with invalid 'how' → EINVAL\n");
    close(s);

    /* --- 7) recv returns 0 on orderly shutdown (EOF) --- */
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);

    shutdown(pair[1], SHUT_WR); /* writer signals EOF */

    ssize_t n = recv(pair[0], buf, sizeof(buf), 0);
    assert(n == 0); /* EOF */
    printf("7. recv after peer SHUT_WR → 0 (EOF)\n");

    close(pair[0]);
    close(pair[1]);

    /* --- 8) send after own SHUT_WR → EPIPE --- */
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, pair) == 0);

    shutdown(pair[0], SHUT_WR);

    errno = 0;
    n = send(pair[0], "x", 1, MSG_NOSIGNAL);
    assert(n == -1);
    assert(errno == EPIPE);
    printf("8. send after own SHUT_WR → EPIPE\n");

    close(pair[0]);
    close(pair[1]);

    printf("All network error handling tests passed\n");
    return 0;
}
