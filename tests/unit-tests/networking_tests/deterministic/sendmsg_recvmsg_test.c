#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/uio.h>
#include <unistd.h>

int main() {
    int sv[2];
    if (socketpair(AF_UNIX, SOCK_DGRAM, 0, sv) == -1) {
        perror("socketpair");
        return 1;
    }

    /* sendmsg with 2 iov entries */
    char *s1 = "hello-";
    char *s2 = "world";
    struct iovec siov[2];
    siov[0].iov_base = s1; siov[0].iov_len = strlen(s1);
    siov[1].iov_base = s2; siov[1].iov_len = strlen(s2);

    struct msghdr smsg;
    memset(&smsg, 0, sizeof(smsg));
    smsg.msg_iov    = siov;
    smsg.msg_iovlen = 2;

    ssize_t ns = sendmsg(sv[0], &smsg, 0);
    if (ns == -1) {
        perror("sendmsg");
        close(sv[0]); close(sv[1]);
        return 1;
    }
    size_t total = strlen(s1) + strlen(s2);
    if ((size_t)ns != total) {
        printf("sendmsg: expected %zu bytes, got %zd\n", total, ns);
        close(sv[0]); close(sv[1]);
        return 1;
    }

    /* recvmsg on the other end */
    char rbuf[64] = {0};
    struct iovec riov[1];
    riov[0].iov_base = rbuf;
    riov[0].iov_len  = sizeof(rbuf) - 1;

    struct msghdr rmsg;
    memset(&rmsg, 0, sizeof(rmsg));
    rmsg.msg_iov    = riov;
    rmsg.msg_iovlen = 1;

    ssize_t nr = recvmsg(sv[1], &rmsg, 0);
    if (nr == -1) {
        perror("recvmsg");
        close(sv[0]); close(sv[1]);
        return 1;
    }
    if ((size_t)nr != total) {
        printf("recvmsg: expected %zu bytes, got %zd\n", total, nr);
        close(sv[0]); close(sv[1]);
        return 1;
    }
    rbuf[nr] = '\0';

    if (strcmp(rbuf, "hello-world") != 0) {
        printf("content mismatch: got [%s]\n", rbuf);
        close(sv[0]); close(sv[1]);
        return 1;
    }

    close(sv[0]);
    close(sv[1]);
    printf("sendmsg_recvmsg_test passed\n");
    return 0;
}
