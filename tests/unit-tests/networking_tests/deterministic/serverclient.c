#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>

#define MSG "Hello from sv0"
#define ECHO "Echo from sv1"

static void fail(const char *reason)
{
    fprintf(stderr, "serverclient: %s\n", reason);
    exit(1);
}

static ssize_t read_exact(int fd, char *buf, size_t len)
{
    size_t total = 0;
    while (total < len) {
        ssize_t n = recv(fd, buf + total, len - total, 0);
        if (n <= 0)
            return n;
        total += (size_t)n;
    }
    return (ssize_t)total;
}

int main(void)
{
    int sv[2];
    char buf[64];
    size_t msg_len = strlen(MSG) + 1;
    size_t echo_len = strlen(ECHO) + 1;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0)
        fail("socketpair failed");

    /* 1) write fixed message from sv[0] */
    if (send(sv[0], MSG, msg_len, 0) != (ssize_t)msg_len)
        fail("send msg failed");

    /* 2) read exact message from sv[1] in a loop */
    if (read_exact(sv[1], buf, msg_len) != (ssize_t)msg_len)
        fail("recv msg failed");

    /* 3) assert content matches */
    if (memcmp(buf, MSG, msg_len) != 0)
        fail("msg content mismatch");

    /* 4) write echo back from sv[1] */
    if (send(sv[1], ECHO, echo_len, 0) != (ssize_t)echo_len)
        fail("send echo failed");

    /* 5) read echo on sv[0] */
    if (read_exact(sv[0], buf, echo_len) != (ssize_t)echo_len)
        fail("recv echo failed");

    /* 6) assert */
    if (memcmp(buf, ECHO, echo_len) != 0)
        fail("echo content mismatch");

    /* 7) close and return 0 */
    close(sv[0]);
    close(sv[1]);
    return 0;
}
