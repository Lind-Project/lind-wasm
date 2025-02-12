// a basic signal test

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

// Custom signal handler
void handle_signal(int signal) {
    printf("Caught signal %d\n", signal);
}

int main() {
    printf("main starts!\n");
    struct sigaction sa;

    sa.sa_handler = handle_signal;
    sa.sa_flags = 0;

    if (sigaction(SIGUSR1, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    int pid = fork();
    if(pid == 0)
    {
        printf("child ready to kill\n");
        kill(getppid(), SIGUSR1);
        printf("child done kill\n");
    }
    else
    {
        while (1) {
            printf("parent in loop, pid=%d\n", getpid());
            sleep(1);
        }
        printf("parent outside loop (should not reach)\n");
    }

    return 0;
}
