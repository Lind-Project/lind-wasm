#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    int fd1 = open("asdfasdf",  O_CREAT | O_RDWR, 0777);
    int fd2 = open("asdfasdf2", O_CREAT | O_RDWR | O_CLOEXEC, 0777);
    if (fd1 < 0 || fd2 < 0) {
        // Keep silent on failure to avoid stderr noise in harness
        return 2;
    }

    int flags1 = fcntl(fd1, F_GETFD);
    int flags2 = fcntl(fd2, F_GETFD);

    // Cleanup (don’t print anything except the final success line)
    close(fd1);
    close(fd2);
    unlink("asdfasdf");
    unlink("asdfasdf2");

    // Validate: fd1 should NOT have FD_CLOEXEC; fd2 SHOULD have FD_CLOEXEC
    if (flags1 >= 0 && (flags1 & FD_CLOEXEC) == 0 &&
        flags2 >= 0 && (flags2 & FD_CLOEXEC) != 0) {
        puts("CLOEXEC flags OK");
        fflush(stdout);
        return 0;
    }
    return 1;
}

