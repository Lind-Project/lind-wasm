// test for sa_mask in sigaction

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

// Custom signal handler
void handle_signal(int signal) {
    printf("Caught signal %d\n", signal);
    int tmp = 2;
    while(tmp--)
    {
        sleep(1);
    }
    printf("signal %d done\n", signal);
}

int main() {
    printf("main starts!\n");
    struct sigaction sa;

    sa.sa_handler = handle_signal;
    sigemptyset(&sa.sa_mask);
    sigaddset(&sa.sa_mask, SIGUSR2);
    sa.sa_flags = 0;

    if (sigaction(SIGUSR1, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    sigemptyset(&sa.sa_mask);
    if (sigaction(SIGUSR2, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    int pid = fork();
    if(pid == 0)
    {
        printf("child ready to kill\n");
        kill(getppid(), SIGUSR1);
        kill(getppid(), SIGUSR2);
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
