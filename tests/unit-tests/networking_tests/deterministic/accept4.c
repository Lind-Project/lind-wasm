#define _GNU_SOURCE
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <string.h>
#include <errno.h>
#include <stdlib.h>
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

int main(void) {
    int s_listen = socket(AF_INET, SOCK_STREAM, 0);
    if (s_listen < 0) return 1;

    int yes = 1;
    setsockopt(s_listen, SOL_SOCKET, SO_REUSEADDR, &yes, sizeof(yes));

    struct sockaddr_in srv = {0};
    srv.sin_family = AF_INET;
    srv.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    srv.sin_port = htons(49160);

    if (bind(s_listen, (struct sockaddr *)&srv, sizeof(srv)) < 0) {
        perror("bind");
        return 1;
    }
    if (listen(s_listen, 1) < 0) {
        perror("listen");
        return 1;
    }

    int s_client = socket(AF_INET, SOCK_STREAM, 0);
    if (s_client < 0) {
        perror("socket");
        return 1;
    }

    if (connect(s_client, (struct sockaddr *)&srv, sizeof(srv)) < 0) {
        perror("connect");
        return 1;
    }

    // Test accept4 with SOCK_CLOEXEC flag
    struct sockaddr_in peer;
    socklen_t peerlen = sizeof(peer);
    int s_conn = accept4(s_listen, (struct sockaddr *)&peer, &peerlen, SOCK_CLOEXEC);
    if (s_conn < 0) {
        perror("accept4");
        return 1;
    }

    // Verify SOCK_CLOEXEC was applied
    int flags = fcntl(s_conn, F_GETFD);
    if (flags < 0 || !(flags & FD_CLOEXEC)) {
        printf("FAIL: SOCK_CLOEXEC not set\n");
        return 1;
    }

    // Verify connection works
    const char msg[] = "hello";
    if (send(s_client, msg, sizeof(msg) - 1, 0) < 0) {
        perror("send");
        return 1;
    }

    char buf[16] = {0};
    if (recv(s_conn, buf, sizeof(buf), 0) <= 0) {
        perror("recv");
        return 1;
    }

    if (memcmp(buf, msg, 5) != 0) {
        printf("FAIL: data mismatch\n");
        return 1;
    }

    close(s_conn);
    close(s_client);
    close(s_listen);
    return 0;
}