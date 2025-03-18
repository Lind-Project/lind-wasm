// test for SA_NODEFER flag for sigaction

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

void handle_signal(int signal) {
    printf("Caught signal %d\n", signal);
    printf("start of signal handler\n");
    int tmp = 2;
    while(tmp--)
    {
        sleep(1);
    }
    // sleep(2);
    printf("end of signal handler\n");
}

int main() {
    printf("main starts!\n");
    struct sigaction sa;

    sa.sa_handler = handle_signal;
    // sa.sa_flags = 0;
    sa.sa_flags = SA_NODEFER;

    if (sigaction(SIGUSR1, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    int ppid = getpid();
    int pid = fork();
    if(pid == 0)
    {
        printf("*child ready to kill\n");
        kill(ppid, SIGUSR1);
        printf("*child sent SIGUSR1\n");
        sleep(1);
        printf("*child ready to send SIGUSR1 again\n");
        kill(ppid, SIGUSR1);
        printf("*child done kill\n");
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
