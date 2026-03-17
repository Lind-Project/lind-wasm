#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <stdio.h>
#include <sys/select.h>
#include <sys/socket.h>

#define MSG "UDS select test message"

static void fail(const char *reason)
{
    fprintf(stderr, "uds-socketselect: %s\n", reason);
    exit(1);
}

int main(void)
{
    int sv[2];
    fd_set readfds;
    struct timeval tv;
    char buf[64];
    size_t msg_len = strlen(MSG) + 1;
    ssize_t n;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0)
        fail("socketpair failed");

    /* Write message from sv[0] */
    if (send(sv[0], MSG, msg_len, 0) != (ssize_t)msg_len)
        fail("send failed");

    /* Select on read end sv[1] */
    FD_ZERO(&readfds);
    FD_SET(sv[1], &readfds);
    tv.tv_sec = 1;
    tv.tv_usec = 0;
    n = select(sv[1] + 1, &readfds, NULL, NULL, &tv);
    if (n != 1)
        fail("select did not return 1");
    if (!FD_ISSET(sv[1], &readfds))
        fail("FD not ready after select");

    /* Read and assert bytes */
    n = recv(sv[1], buf, sizeof(buf), 0);
    if (n != (ssize_t)msg_len)
        fail("recv byte count mismatch");
    if (memcmp(buf, MSG, msg_len) != 0)
        fail("recv content mismatch");

    close(sv[0]);
    close(sv[1]);
    return 0;
}
