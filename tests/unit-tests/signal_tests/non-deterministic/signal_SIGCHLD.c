// signal test for SIGCHLD signal when child exits

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

int sigchld_received = 0;

// Custom signal handler
void handle_signal(int signal) {
    printf("Caught signal %d\n", signal);
    if(signal == SIGCHLD) sigchld_received = 1;
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

    if (sigaction(SIGCHLD, &sa, NULL) == -1) {
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
        while (!sigchld_received) {
            printf("parent in loop, pid=%d\n", getpid());
            sleep(1);
        }
    }

    return 0;
}
