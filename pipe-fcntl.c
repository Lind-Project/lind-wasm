// test_self_pipe_fcntl_badfd.c
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <string.h>

int main(void) {
    int pipefd[2];

    printf("[*] Creating self-pipe...\n");

    if (pipe(pipefd) < 0) {
        fprintf(stderr, "pipe() failed: %s\n", strerror(errno));
        return 1;
    }

    int read_fd = pipefd[0];
    int write_fd = pipefd[1];

    printf("[*] pipe read_fd=%d write_fd=%d\n", read_fd, write_fd);

    /*
     * Simulate the failure mode:
     * PostgreSQL/libpq-style self-pipe setup expects read_fd to be valid here.
     * If Lind fd-table state is wrong, this fcntl may fail with EBADF.
     *
     * We intentionally close read_fd to make the expected error explicit.
     */
    printf("[*] Closing read end to simulate broken self-pipe fd...\n");
    close(read_fd);

    printf("[*] Calling fcntl(F_GETFL) on read end...\n");

    int flags = fcntl(read_fd, F_GETFL, 0);
    if (flags < 0) {
        fprintf(stderr,
                "FATAL: fcntl(F_GETFL) failed on read-end of self-pipe: %s\n",
                strerror(errno));
    } else {
        printf("[*] F_GETFL returned flags=%d\n", flags);
    }

    printf("[*] Calling fcntl(F_SETFL) on read end...\n");

    if (fcntl(read_fd, F_SETFL, flags | O_NONBLOCK) < 0) {
        fprintf(stderr,
                "FATAL: fcntl(F_SETFL) failed on read-end of self-pipe: %s\n",
                strerror(errno));
        close(write_fd);
        return 2;
    }

    printf("[+] fcntl(F_SETFL) unexpectedly succeeded\n");

    close(write_fd);
    return 0;
}
