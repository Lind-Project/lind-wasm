// test for doing recursive signal interruption

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

void handle_signal3(int signal) {
    printf("---Caught signal %d\n", signal);
    printf("---start of signal handler 3\n");
    sleep(2);
    fork();
    printf("---end of signal handler 3\n");
}

void handle_signal2(int signal) {
    printf("--Caught signal %d\n", signal);
    printf("--start of signal handler 2\n");
    sleep(2);
    printf("--end of signal handler 2\n");
}

void handle_signal1(int signal) {
    printf("-Caught signal %d\n", signal);
    printf("-start of signal handler 1\n");
    sleep(2);
    printf("-end of signal handler 1\n");
}

int main() {
    printf("main starts!\n");
    struct sigaction sa;

    sa.sa_handler = handle_signal1;
    sa.sa_flags = 0;

    if (sigaction(SIGUSR1, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    sa.sa_handler = handle_signal2;
    if (sigaction(SIGUSR2, &sa, NULL) == -1) {
        perror("sigaction");
        exit(EXIT_FAILURE);
    }

    sa.sa_handler = handle_signal3;
    if (sigaction(SIGINT, &sa, NULL) == -1) {
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
        printf("*child ready to send SIGUSR2\n");
        kill(ppid, SIGUSR2);
        printf("*child sent SIGUSR2\n");
        sleep(2);
        printf("*child ready to send SIGINT\n");
        kill(ppid, SIGINT);
        printf("*child done kill\n");
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
