#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/wait.h>

/*
 * UDP unit test: validates socket, bind, sendto, recvfrom for UDP datagrams.
 * Uses fork so server and client run in the same process tree.
 *
 * Server (child): bind → recvfrom → sendto (echo back)
 * Client (parent): sendto → recvfrom (get echo)
 *
 * This exercises the core UDP path used by lmbench's lat_udp.
 */

#define PORT 19876
#define MSG "Hello UDP from lind-wasm"
#define BUFSIZE 256

static void server(void)
{
    int sock;
    struct sockaddr_in servaddr, cliaddr;
    socklen_t clilen;
    char buf[BUFSIZE];
    ssize_t n;

    sock = socket(AF_INET, SOCK_DGRAM, 0);
    assert(sock >= 0);

    memset(&servaddr, 0, sizeof(servaddr));
    servaddr.sin_family = AF_INET;
    servaddr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    servaddr.sin_port = htons(PORT);

    assert(bind(sock, (struct sockaddr *)&servaddr, sizeof(servaddr)) == 0);

    /* Receive one datagram */
    clilen = sizeof(cliaddr);
    n = recvfrom(sock, buf, BUFSIZE, 0, (struct sockaddr *)&cliaddr, &clilen);
    assert(n > 0);
    buf[n] = '\0';
    assert(strcmp(buf, MSG) == 0);

    /* Echo it back */
    n = sendto(sock, buf, (size_t)n, 0, (struct sockaddr *)&cliaddr, clilen);
    assert(n == (ssize_t)strlen(MSG));

    close(sock);
    _exit(0);
}

int main(void)
{
    int sock;
    struct sockaddr_in servaddr, fromaddr;
    socklen_t fromlen;
    char buf[BUFSIZE];
    ssize_t n;
    pid_t pid;
    int status;

    /* Test 1: Basic UDP socket creation */
    sock = socket(AF_INET, SOCK_DGRAM, 0);
    assert(sock >= 0);

    /* Test 2: Verify socket type is SOCK_DGRAM */
    int socktype;
    socklen_t optlen = sizeof(socktype);
    assert(getsockopt(sock, SOL_SOCKET, SO_TYPE, &socktype, &optlen) == 0);
    assert(socktype == SOCK_DGRAM);

    close(sock);

    /* Test 3: Fork-based server/client UDP echo */
    pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        server();
        /* not reached */
    }

    /* Parent: client */
    /* Give the child time to bind */
    usleep(100000);

    sock = socket(AF_INET, SOCK_DGRAM, 0);
    assert(sock >= 0);

    memset(&servaddr, 0, sizeof(servaddr));
    servaddr.sin_family = AF_INET;
    servaddr.sin_port = htons(PORT);
    servaddr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);

    /* Send datagram to server */
    n = sendto(sock, MSG, strlen(MSG), 0,
               (struct sockaddr *)&servaddr, sizeof(servaddr));
    assert(n == (ssize_t)strlen(MSG));

    /* Receive echo */
    fromlen = sizeof(fromaddr);
    n = recvfrom(sock, buf, BUFSIZE, 0,
                 (struct sockaddr *)&fromaddr, &fromlen);
    assert(n == (ssize_t)strlen(MSG));
    buf[n] = '\0';
    assert(strcmp(buf, MSG) == 0);

    /* Verify sender address */
    assert(fromaddr.sin_family == AF_INET);
    assert(fromaddr.sin_port == htons(PORT));

    close(sock);

    /* Wait for child */
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);

    printf("All UDP tests passed\n");
    return 0;
}
