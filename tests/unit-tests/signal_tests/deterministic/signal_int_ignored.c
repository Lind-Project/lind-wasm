#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>

int main(void) {
    pid_t child = fork();
    if (child < 0) {
        perror("fork");
        return 1;
    }

    if (child == 0) {
        // Child: send SIGCHLD "spontaneously" while still alive.
        pid_t ppid = getppid();

        sleep(1);
        if (kill(ppid, SIGCHLD) == -1) {
            perror("child kill(SIGCHLD)");
        }

        // Keep running a bit so parent is genuinely waiting for our exit.
        sleep(1);
        _exit(0);
    }

    int status;
    for (;;) {
        pid_t r = waitpid(child, &status, 0);
        if (r == -1) {
            perror("waitpid");
            assert("waitpid interrupted");
            return 1;
        }

        assert(status == 0);
        break;
    }

    printf("Test Passed\n");

    return 0;
}
