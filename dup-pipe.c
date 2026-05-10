#include <unistd.h>
#include <sys/wait.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>
#include <stdlib.h>

int main(void) {
    int p[2];

    if (pipe(p) < 0) {
        fprintf(stderr, "pipe failed: %s\n", strerror(errno));
        return 1;
    }

    fprintf(stderr, "parent: pipe read fd=%d, write fd=%d\n", p[0], p[1]);

    pid_t pid = fork();
    if (pid < 0) {
        fprintf(stderr, "fork failed: %s\n", strerror(errno));
        return 1;
    }

    if (pid == 0) {
        const char *msg = "hello-from-child\n";

        close(p[0]);

        /*
         * This is the operation that shell command substitution needs:
         * redirect command stdout to the pipe write end.
         */
        if (dup2(p[1], STDOUT_FILENO) < 0) {
            fprintf(stderr,
                    "command_substitute: cannot duplicate pipe as fd 1: %s\n",
                    strerror(errno));
            _exit(127);
        }

        close(p[1]);

        if (write(STDOUT_FILENO, msg, strlen(msg)) < 0) {
            fprintf(stderr, "child: write(1) failed after dup2: %s\n",
                    strerror(errno));
            _exit(126);
        }

        _exit(0);
    }

    close(p[1]);

    char buf[256];
    ssize_t n = read(p[0], buf, sizeof(buf) - 1);
    if (n < 0) {
        fprintf(stderr, "parent: read(pipe) failed: %s\n", strerror(errno));
        return 1;
    }

    buf[n] = '\0';
    fprintf(stderr, "parent: got from child stdout pipe: [%s]\n", buf);

    int status;
    if (waitpid(pid, &status, 0) < 0) {
        fprintf(stderr, "waitpid failed: %s\n", strerror(errno));
        return 1;
    }

    if (!WIFEXITED(status)) {
        fprintf(stderr, "child did not exit normally\n");
        return 1;
    }

    int code = WEXITSTATUS(status);
    if (code != 0) {
        fprintf(stderr, "child failed with exit code %d\n", code);
        return 1;
    }

    fprintf(stderr, "success\n");
    return 0;
}
