#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static void die(const char *msg) {
    fprintf(stderr, "%s: %s\n", msg, strerror(errno));
    exit(1);
}

int main(void) {
    int pipefd[2];

    if (pipe(pipefd) < 0) {
        die("pipe failed");
    }

    printf("[parent] created pipe: read fd=%d, write fd=%d\n",
           pipefd[0], pipefd[1]);

    pid_t pid = fork();
    if (pid < 0) {
        die("fork failed");
    }

    if (pid == 0) {
        /*
         * Child process.
         *
         * Important: child does NOT call pipe(), open(), dup(), etc.
         * It only uses the fd inherited from parent before fork.
         */
        const char *msg = "hello from child through inherited fd\n";

        close(pipefd[0]);

        ssize_t n = write(pipefd[1], msg, strlen(msg));
        if (n < 0) {
            fprintf(stderr, "[child] write to inherited fd failed: %s\n",
                    strerror(errno));
            _exit(2);
        }

        if ((size_t)n != strlen(msg)) {
            fprintf(stderr, "[child] short write: %zd\n", n);
            _exit(3);
        }

        printf("[child] successfully wrote through inherited fd %d\n",
               pipefd[1]);

        close(pipefd[1]);
        _exit(0);
    }

    /*
     * Parent process.
     */
    close(pipefd[1]);

    char buf[256];
    memset(buf, 0, sizeof(buf));

    ssize_t n = read(pipefd[0], buf, sizeof(buf) - 1);
    if (n < 0) {
        die("[parent] read failed");
    }

    close(pipefd[0]);

    int status = 0;
    if (waitpid(pid, &status, 0) < 0) {
        die("waitpid failed");
    }

    printf("[parent] read %zd bytes: %s", n, buf);

    if (!WIFEXITED(status)) {
        fprintf(stderr, "[parent] child did not exit normally\n");
        return 1;
    }

    int code = WEXITSTATUS(status);
    printf("[parent] child exit code: %d\n", code);

    const char *expected = "hello from child through inherited fd\n";

    if (code != 0) {
        fprintf(stderr, "TEST FAILED: child failed\n");
        return 1;
    }

    if ((size_t)n != strlen(expected) || strcmp(buf, expected) != 0) {
        fprintf(stderr, "TEST FAILED: parent did not receive expected data\n");
        return 1;
    }

    printf("TEST PASSED: child inherited parent's fd table after fork\n");
    return 0;
}