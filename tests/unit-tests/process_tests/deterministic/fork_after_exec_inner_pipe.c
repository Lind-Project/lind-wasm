/*
 * fork_after_exec_inner_pipe.c — Inner binary with pipe coordination.
 *
 * Like fork_after_exec_inner.c but adds pipe-based parent-child
 * communication (mimics benchmp's pipe handshake).
 *
 * Flow: fork → child writes "ready" to pipe → parent reads → both exit
 *
 * Used by: fork_after_exec_outer.c (swap the exec path to test this)
 * Also runnable standalone.
 *
 * Expected output:
 *   inner_pipe: creating pipe
 *   inner_pipe: forking
 *   inner_pipe: child writing to pipe
 *   inner_pipe: parent reading from pipe
 *   inner_pipe: parent got: ready
 *   inner_pipe: child exiting
 *   inner_pipe: waitpid returned
 *   inner_pipe: done
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

int main() {
    int pfd[2];

    fprintf(stderr, "inner_pipe: creating pipe\n");
    if (pipe(pfd) < 0) {
        perror("inner_pipe: pipe failed");
        return 1;
    }

    fprintf(stderr, "inner_pipe: forking\n");
    pid_t pid = fork();
    if (pid < 0) {
        perror("inner_pipe: fork failed");
        return 1;
    }

    if (pid == 0) {
        /* child: write to pipe */
        close(pfd[0]);
        fprintf(stderr, "inner_pipe: child writing to pipe\n");
        const char *msg = "ready";
        write(pfd[1], msg, strlen(msg));
        close(pfd[1]);
        fprintf(stderr, "inner_pipe: child exiting\n");
        _exit(0);
    }

    /* parent: read from pipe */
    close(pfd[1]);
    fprintf(stderr, "inner_pipe: parent reading from pipe\n");
    char buf[32] = {0};
    int n = read(pfd[0], buf, sizeof(buf) - 1);
    close(pfd[0]);

    if (n > 0) {
        fprintf(stderr, "inner_pipe: parent got: %s\n", buf);
    } else {
        fprintf(stderr, "inner_pipe: parent read returned %d\n", n);
    }

    int status;
    waitpid(pid, &status, 0);
    fprintf(stderr, "inner_pipe: waitpid returned\n");
    fprintf(stderr, "inner_pipe: done\n");
    return 0;
}
