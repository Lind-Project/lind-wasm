// test doing fork inside signal handler

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

// Custom signal handler
void handle_signal(int signal) {
    printf("Caught signal %d\n", signal);
    int pid = fork();
    printf("after fork inside signal handler, pid=%d\n", getpid());
}

int main() {
    printf("main starts!\n");
    struct sigaction sa;

    sa.sa_handler = handle_signal;
    sa.sa_flags = 0;

    if (sigaction(SIGINT, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    int pid = fork();
    if(pid == 0)
    {
        printf("child ready to kill\n");
        kill(getppid(), SIGINT);
        printf("child done kill\n");
    }
    else
    {
        int counter = 5;
        while (counter--) {
            printf("parent in loop, pid=%d\n", getpid());
            sleep(1);
        }
    }

    return 0;
}
