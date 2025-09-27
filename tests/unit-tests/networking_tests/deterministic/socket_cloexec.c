#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <fcntl.h>
#include <errno.h>

int main(void) {
    int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) {
        perror("socket");
        return 1;
    }

    int flags = fcntl(fd, F_GETFD);
    if (flags < 0) {
        perror("fcntl(F_GETFD)");
        close(fd);
        return 1;
    }

    if (flags & FD_CLOEXEC) {
        printf("SOCK_CLOEXEC is set.\n");
    } else {
        printf("SOCK_CLOEXEC is NOT set.\n");
    }

    close(fd);
    return 0;
}
