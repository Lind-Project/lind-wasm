/*
 * UDP unit test for lind-wasm.
 *
 * Tests are ordered to isolate failures incrementally:
 *   1. socket creation
 *   2. socket type verification
 *   3. bind
 *   4. sendto
 *   5. recvfrom with NULL src_addr (no sender info requested)
 *   6. recvfrom with non-NULL src_addr (sender info requested)
 *   7. fork-based server/client echo (full round-trip)
 */

#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/wait.h>

#define PORT 19876
#define MSG "Hello UDP from lind-wasm"
#define BUFSIZE 256

int main(void)
{
    int sock;
    struct sockaddr_in servaddr, fromaddr;
    socklen_t fromlen;
    char buf[BUFSIZE];
    ssize_t n;

    /* 1. UDP socket creation */
    sock = socket(AF_INET, SOCK_DGRAM, 0);
    assert(sock >= 0);

    /* 2. Verify socket type */
    int socktype;
    socklen_t optlen = sizeof(socktype);
    assert(getsockopt(sock, SOL_SOCKET, SO_TYPE, &socktype, &optlen) == 0);
    assert(socktype == SOCK_DGRAM);

    /* 3. Bind to loopback */
    memset(&servaddr, 0, sizeof(servaddr));
    servaddr.sin_family = AF_INET;
    servaddr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    servaddr.sin_port = htons(PORT);
    assert(bind(sock, (struct sockaddr *)&servaddr, sizeof(servaddr)) == 0);

    /* 4. sendto */
    n = sendto(sock, MSG, strlen(MSG), 0,
               (struct sockaddr *)&servaddr, sizeof(servaddr));
    assert(n == (ssize_t)strlen(MSG));

    /* 5. recvfrom with NULL src_addr — does NOT call copy_out_sockaddr */
    n = recvfrom(sock, buf, BUFSIZE, 0, NULL, NULL);
    assert(n == (ssize_t)strlen(MSG));
    buf[n] = '\0';
    assert(strcmp(buf, MSG) == 0);

    /* Send again for test 6 */
    n = sendto(sock, MSG, strlen(MSG), 0,
               (struct sockaddr *)&servaddr, sizeof(servaddr));
    assert(n == (ssize_t)strlen(MSG));

    /* 6. recvfrom with non-NULL src_addr — calls copy_out_sockaddr */
    fromlen = sizeof(fromaddr);
    n = recvfrom(sock, buf, BUFSIZE, 0,
                 (struct sockaddr *)&fromaddr, &fromlen);
    assert(n == (ssize_t)strlen(MSG));
    buf[n] = '\0';
    assert(strcmp(buf, MSG) == 0);
    assert(fromaddr.sin_family == AF_INET);
    assert(fromaddr.sin_port == htons(PORT));

    close(sock);

    /* 7. Fork-based server/client echo */
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        int ssock = socket(AF_INET, SOCK_DGRAM, 0);
        assert(ssock >= 0);

        struct sockaddr_in saddr, caddr;
        memset(&saddr, 0, sizeof(saddr));
        saddr.sin_family = AF_INET;
        saddr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        saddr.sin_port = htons(PORT + 1);
        assert(bind(ssock, (struct sockaddr *)&saddr, sizeof(saddr)) == 0);

        socklen_t clen = sizeof(caddr);
        ssize_t rn = recvfrom(ssock, buf, BUFSIZE, 0,
                              (struct sockaddr *)&caddr, &clen);
        assert(rn > 0);
        assert(sendto(ssock, buf, (size_t)rn, 0,
                      (struct sockaddr *)&caddr, clen) == rn);

        close(ssock);
        _exit(0);
    }

    usleep(100000);

    int csock = socket(AF_INET, SOCK_DGRAM, 0);
    assert(csock >= 0);

    struct sockaddr_in dest;
    memset(&dest, 0, sizeof(dest));
    dest.sin_family = AF_INET;
    dest.sin_port = htons(PORT + 1);
    dest.sin_addr.s_addr = htonl(INADDR_LOOPBACK);

    n = sendto(csock, MSG, strlen(MSG), 0,
               (struct sockaddr *)&dest, sizeof(dest));
    assert(n == (ssize_t)strlen(MSG));

    fromlen = sizeof(fromaddr);
    n = recvfrom(csock, buf, BUFSIZE, 0,
                 (struct sockaddr *)&fromaddr, &fromlen);
    assert(n == (ssize_t)strlen(MSG));
    buf[n] = '\0';
    assert(strcmp(buf, MSG) == 0);

    close(csock);

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);

    printf("All UDP tests passed\n");
    return 0;
}
