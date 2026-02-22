/*
 * IPv6 basic socket operations: create, bind, listen, connect, send/recv
 * on the IPv6 loopback address (::1).
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <errno.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>

#define PORT 49200

int main(void) {
    /* --- 1) Create IPv6 TCP socket --- */
    int srv = socket(AF_INET6, SOCK_STREAM, 0);
    assert(srv >= 0);
    printf("1. AF_INET6 TCP socket created\n");

    /* Verify SO_TYPE */
    int stype;
    socklen_t slen = sizeof(stype);
    assert(getsockopt(srv, SOL_SOCKET, SO_TYPE, &stype, &slen) == 0);
    assert(stype == SOCK_STREAM);

    /* --- 2) Bind to [::1]:PORT --- */
    int yes = 1;
    assert(setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes)) == 0);

    struct sockaddr_in6 addr = {0};
    addr.sin6_family = AF_INET6;
    addr.sin6_port = htons(PORT);
    addr.sin6_addr = in6addr_loopback; /* ::1 */

    assert(bind(srv, (struct sockaddr *)&addr, sizeof(addr)) == 0);
    printf("2. Bound to [::1]:%d\n", PORT);

    /* --- 3) getsockname round-trip --- */
    struct sockaddr_in6 bound = {0};
    socklen_t blen = sizeof(bound);
    assert(getsockname(srv, (struct sockaddr *)&bound, &blen) == 0);
    assert(bound.sin6_family == AF_INET6);
    assert(ntohs(bound.sin6_port) == PORT);
    assert(memcmp(&bound.sin6_addr, &in6addr_loopback, sizeof(struct in6_addr)) == 0);
    printf("3. getsockname matches [::1]:%d\n", PORT);

    /* --- 4) Listen --- */
    assert(listen(srv, 1) == 0);

    /* --- 5) Client: connect to [::1]:PORT --- */
    int cli = socket(AF_INET6, SOCK_STREAM, 0);
    assert(cli >= 0);
    assert(connect(cli, (struct sockaddr *)&addr, sizeof(addr)) == 0);
    printf("4. Client connected\n");

    /* --- 6) Accept --- */
    struct sockaddr_in6 peer = {0};
    socklen_t plen = sizeof(peer);
    int conn = accept(srv, (struct sockaddr *)&peer, &plen);
    assert(conn >= 0);
    assert(peer.sin6_family == AF_INET6);
    printf("5. Server accepted (peer port %d)\n", ntohs(peer.sin6_port));

    /* --- 7) getpeername --- */
    struct sockaddr_in6 pn = {0};
    socklen_t pnlen = sizeof(pn);
    assert(getpeername(cli, (struct sockaddr *)&pn, &pnlen) == 0);
    assert(pn.sin6_family == AF_INET6);
    assert(ntohs(pn.sin6_port) == PORT);
    printf("6. getpeername OK\n");

    /* --- 8) Send/recv --- */
    const char *msg = "ipv6 hello";
    ssize_t n = send(cli, msg, strlen(msg), 0);
    assert(n == (ssize_t)strlen(msg));

    char buf[64] = {0};
    n = recv(conn, buf, sizeof(buf), 0);
    assert(n == (ssize_t)strlen(msg));
    assert(memcmp(buf, msg, strlen(msg)) == 0);
    printf("7. send/recv OK: \"%s\"\n", buf);

    /* --- 9) IPv6 UDP socket --- */
    int udp = socket(AF_INET6, SOCK_DGRAM, 0);
    assert(udp >= 0);

    slen = sizeof(stype);
    assert(getsockopt(udp, SOL_SOCKET, SO_TYPE, &stype, &slen) == 0);
    assert(stype == SOCK_DGRAM);
    printf("8. AF_INET6 UDP socket OK\n");

    close(udp);
    close(conn);
    close(cli);
    close(srv);

    printf("All IPv6 tests passed\n");
    return 0;
}
