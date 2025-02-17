// test for doing longjmp inside signal handler

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <setjmp.h>

jmp_buf jump_buffer;

// Custom signal handler
void handle_signal(int signal) {
    printf("Caught signal %d\n", signal);
    int pid = fork();
    printf("after fork inside signal handler, pid=%d\n", getpid());
    if(pid == 0)
    {
        longjmp(jump_buffer, 42);
    }
    printf("after child longjmp (should only be printed once by parent)\n");
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

    int val = setjmp(jump_buffer);
    if(val != 0)
    {
        printf("back from setjmp: %d! pid=%d\n", val, getpid());
        return 0;
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
        int counter = 5;
        while (counter--) {
            printf("parent in loop, pid=%d\n", getpid());
            sleep(1);
        }
    }

    return 0;
}
