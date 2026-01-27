#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

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
        printf("child: sending SIGCHLD to parent (without exiting)\n");
        if (kill(ppid, SIGCHLD) == -1) {
            perror("child kill(SIGCHLD)");
        }

        // Keep running a bit so parent is genuinely waiting for our exit.
        sleep(2);
        printf("child: now exiting\n");
        _exit(0);
    }

    // Parent
    printf("parent: pid=%ld, child=%ld\n", (long)getpid(), (long)child);

    int status;
    for (;;) {
        printf("parent: calling waitpid() (will block)...\n");
        pid_t r = waitpid(child, &status, 0);
        if (r == -1) {
            if (errno == EINTR) {
                printf("parent: waitpid() interrupted by signal (EINTR)\n");
            }
            perror("waitpid");
            return 1;
        }

        printf("parent: waitpid returned\n");
        if (WIFEXITED(status)) {
            printf("parent: child exited with status %d\n", WEXITSTATUS(status));
        } else if (WIFSIGNALED(status)) {
            printf("parent: child killed by signal %d\n", WTERMSIG(status));
        }
        break;
    }

    return 0;
}
