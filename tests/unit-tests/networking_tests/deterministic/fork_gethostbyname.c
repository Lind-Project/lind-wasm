/*
 * fork_gethostbyname.c — Test gethostbyname in a forked child
 *
 * Tests whether gethostbyname("127.0.0.1") crashes in a forked child.
 * This isolates the suspected cause of lat_tcp client benchmp crash.
 */
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <netdb.h>
#include <sys/wait.h>

int main(void) {
    /* Test in parent first */
    fprintf(stderr, "PARENT: calling gethostbyname(\"127.0.0.1\")...\n");
    struct hostent *h = gethostbyname("127.0.0.1");
    if (h) {
        fprintf(stderr, "PARENT: gethostbyname OK (name=%s)\n", h->h_name);
    } else {
        fprintf(stderr, "PARENT: gethostbyname failed\n");
    }

    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }

    if (pid == 0) {
        /* Child: same call */
        fprintf(stderr, "CHILD: calling gethostbyname(\"127.0.0.1\")...\n");
        h = gethostbyname("127.0.0.1");
        if (h) {
            fprintf(stderr, "CHILD: gethostbyname OK (name=%s)\n", h->h_name);
        } else {
            fprintf(stderr, "CHILD: gethostbyname failed\n");
        }
        printf("PASS: gethostbyname works in forked child\n");
        _exit(0);
    }

    int status;
    waitpid(pid, &status, 0);
    if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
        fprintf(stderr, "PARENT: child exited OK\n");
    } else {
        printf("FAIL: child crashed or exited with %d\n", WEXITSTATUS(status));
    }
    return 0;
}
