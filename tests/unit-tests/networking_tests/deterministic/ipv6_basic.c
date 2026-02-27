/*
 * IPv6 basic socket operations: create, bind, listen, connect, send/recv.
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
    /* Create IPv6 TCP socket */
    int srv = socket(AF_INET6, SOCK_STREAM, 0);
    assert(srv >= 0);

    int stype;
    socklen_t slen = sizeof(stype);
    assert(getsockopt(srv, SOL_SOCKET, SO_TYPE, &stype, &slen) == 0);
    assert(stype == SOCK_STREAM);

    /* Bind to [::]:PORT */
    int yes = 1;
    assert(setsockopt(srv, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes)) == 0);

    struct sockaddr_in6 addr = {0};
    addr.sin6_family = AF_INET6;
    addr.sin6_port = htons(PORT);
    addr.sin6_addr = in6addr_any;

    assert(bind(srv, (struct sockaddr *)&addr, sizeof(addr)) == 0);

    /* getsockname round-trip */
    struct sockaddr_in6 bound = {0};
    socklen_t blen = sizeof(bound);
    assert(getsockname(srv, (struct sockaddr *)&bound, &blen) == 0);
    assert(bound.sin6_family == AF_INET6);
    assert(ntohs(bound.sin6_port) == PORT);

    /* Listen */
    assert(listen(srv, 1) == 0);

    /* Client: connect via IPv4-mapped address (::ffff:127.0.0.1) */
    int cli = socket(AF_INET6, SOCK_STREAM, 0);
    assert(cli >= 0);

    struct sockaddr_in6 dst = {0};
    dst.sin6_family = AF_INET6;
    dst.sin6_port = htons(PORT);
    unsigned char mapped[] = {0,0,0,0, 0,0,0,0, 0,0,0xff,0xff, 127,0,0,1};
    memcpy(&dst.sin6_addr, mapped, 16);

    assert(connect(cli, (struct sockaddr *)&dst, sizeof(dst)) == 0);

    /* Accept */
    struct sockaddr_in6 peer = {0};
    socklen_t plen = sizeof(peer);
    int conn = accept(srv, (struct sockaddr *)&peer, &plen);
    assert(conn >= 0);
    assert(peer.sin6_family == AF_INET6);

    /* Send/recv */
    const char *msg = "ipv6 hello";
    ssize_t n = send(cli, msg, strlen(msg), 0);
    assert(n == (ssize_t)strlen(msg));

    char buf[64] = {0};
    n = recv(conn, buf, sizeof(buf), 0);
    assert(n == (ssize_t)strlen(msg));
    assert(memcmp(buf, msg, strlen(msg)) == 0);

    /* getpeername on client */
    struct sockaddr_in6 pn = {0};
    socklen_t pnlen = sizeof(pn);
    assert(getpeername(cli, (struct sockaddr *)&pn, &pnlen) == 0);
    assert(pn.sin6_family == AF_INET6);
    assert(ntohs(pn.sin6_port) == PORT);

    /* IPv6 UDP socket */
    int udp = socket(AF_INET6, SOCK_DGRAM, 0);
    assert(udp >= 0);

    slen = sizeof(stype);
    assert(getsockopt(udp, SOL_SOCKET, SO_TYPE, &stype, &slen) == 0);
    assert(stype == SOCK_DGRAM);

    close(udp);
    close(conn);
    close(cli);
    close(srv);

    printf("All IPv6 tests passed\n");
    return 0;
}
