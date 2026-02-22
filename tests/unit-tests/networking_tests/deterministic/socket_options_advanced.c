/*
 * Advanced socket option tests: TCP_NODELAY, SO_LINGER, SO_SNDBUF/RCVBUF,
 * SO_RCVTIMEO/SO_SNDTIMEO, SO_REUSEPORT, SO_ERROR.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <errno.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>

int main(void) {
    int tcp = socket(AF_INET, SOCK_STREAM, 0);
    assert(tcp >= 0);

    int val;
    socklen_t len;

    /* --- 1) TCP_NODELAY --- */
    len = sizeof(val);
    assert(getsockopt(tcp, IPPROTO_TCP, TCP_NODELAY, &val, &len) == 0);
    assert(val == 0); /* default off */

    val = 1;
    assert(setsockopt(tcp, IPPROTO_TCP, TCP_NODELAY, &val, sizeof(val)) == 0);

    val = 0;
    len = sizeof(val);
    assert(getsockopt(tcp, IPPROTO_TCP, TCP_NODELAY, &val, &len) == 0);
    assert(val == 1);
    printf("1. TCP_NODELAY round-trip OK\n");

    /* --- 2) SO_LINGER --- */
    struct linger lg = {0};
    len = sizeof(lg);
    assert(getsockopt(tcp, SOL_SOCKET, SO_LINGER, &lg, &len) == 0);
    assert(lg.l_onoff == 0); /* default off */

    lg.l_onoff = 1;
    lg.l_linger = 5;
    assert(setsockopt(tcp, SOL_SOCKET, SO_LINGER, &lg, sizeof(lg)) == 0);

    struct linger lg2 = {0};
    len = sizeof(lg2);
    assert(getsockopt(tcp, SOL_SOCKET, SO_LINGER, &lg2, &len) == 0);
    assert(lg2.l_onoff != 0);
    assert(lg2.l_linger == 5);
    printf("2. SO_LINGER round-trip OK (linger=%ds)\n", lg2.l_linger);

    /* --- 3) SO_SNDBUF / SO_RCVBUF --- */
    int sndbuf;
    len = sizeof(sndbuf);
    assert(getsockopt(tcp, SOL_SOCKET, SO_SNDBUF, &sndbuf, &len) == 0);
    assert(sndbuf > 0);
    printf("3a. SO_SNDBUF default = %d\n", sndbuf);

    int rcvbuf;
    len = sizeof(rcvbuf);
    assert(getsockopt(tcp, SOL_SOCKET, SO_RCVBUF, &rcvbuf, &len) == 0);
    assert(rcvbuf > 0);
    printf("3b. SO_RCVBUF default = %d\n", rcvbuf);

    /* Set and read back — kernel may double the value */
    int want = 32768;
    assert(setsockopt(tcp, SOL_SOCKET, SO_SNDBUF, &want, sizeof(want)) == 0);
    len = sizeof(sndbuf);
    assert(getsockopt(tcp, SOL_SOCKET, SO_SNDBUF, &sndbuf, &len) == 0);
    assert(sndbuf >= want); /* kernel doubles it */
    printf("3c. SO_SNDBUF set %d → got %d\n", want, sndbuf);

    assert(setsockopt(tcp, SOL_SOCKET, SO_RCVBUF, &want, sizeof(want)) == 0);
    len = sizeof(rcvbuf);
    assert(getsockopt(tcp, SOL_SOCKET, SO_RCVBUF, &rcvbuf, &len) == 0);
    assert(rcvbuf >= want);
    printf("3d. SO_RCVBUF set %d → got %d\n", want, rcvbuf);

    /* --- 4) SO_RCVTIMEO / SO_SNDTIMEO --- */
    struct timeval tv = { .tv_sec = 2, .tv_usec = 500000 };
    assert(setsockopt(tcp, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv)) == 0);

    struct timeval tv2 = {0};
    len = sizeof(tv2);
    assert(getsockopt(tcp, SOL_SOCKET, SO_RCVTIMEO, &tv2, &len) == 0);
    assert(tv2.tv_sec == 2);
    assert(tv2.tv_usec == 500000);
    printf("4a. SO_RCVTIMEO round-trip OK (2.5s)\n");

    tv.tv_sec = 3;
    tv.tv_usec = 0;
    assert(setsockopt(tcp, SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof(tv)) == 0);

    memset(&tv2, 0, sizeof(tv2));
    len = sizeof(tv2);
    assert(getsockopt(tcp, SOL_SOCKET, SO_SNDTIMEO, &tv2, &len) == 0);
    assert(tv2.tv_sec == 3);
    printf("4b. SO_SNDTIMEO round-trip OK (3s)\n");

    /* --- 5) SO_REUSEPORT --- */
    val = 1;
    assert(setsockopt(tcp, SOL_SOCKET, SO_REUSEPORT, &val, sizeof(val)) == 0);

    val = 0;
    len = sizeof(val);
    assert(getsockopt(tcp, SOL_SOCKET, SO_REUSEPORT, &val, &len) == 0);
    assert(val == 1);
    printf("5. SO_REUSEPORT round-trip OK\n");

    /* --- 6) SO_ERROR (read-only, clears pending error) --- */
    int err;
    len = sizeof(err);
    assert(getsockopt(tcp, SOL_SOCKET, SO_ERROR, &err, &len) == 0);
    assert(err == 0); /* no pending error */
    printf("6. SO_ERROR = 0 (no error)\n");

    /* --- 7) SO_ACCEPTCONN (read-only) --- */
    int acc;
    len = sizeof(acc);
    assert(getsockopt(tcp, SOL_SOCKET, SO_ACCEPTCONN, &acc, &len) == 0);
    assert(acc == 0); /* not listening */

    /* Make it listen, then check again */
    int yes = 1;
    assert(setsockopt(tcp, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes)) == 0);

    struct sockaddr_in a = {0};
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    a.sin_port = htons(49210);
    assert(bind(tcp, (struct sockaddr *)&a, sizeof(a)) == 0);
    assert(listen(tcp, 1) == 0);

    len = sizeof(acc);
    assert(getsockopt(tcp, SOL_SOCKET, SO_ACCEPTCONN, &acc, &len) == 0);
    assert(acc == 1); /* now listening */
    printf("7. SO_ACCEPTCONN: 0 before listen, 1 after\n");

    close(tcp);

    printf("All advanced socket option tests passed\n");
    return 0;
}
