// test for SA_RESETHAND flag for sigaction

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

void handler(int sig) {
    printf("Signal %d received! Handler is running...\n", sig);
    printf("After this, the handler will reset to default behavior.\n");
}

int main() {
    struct sigaction sa;
    sa.sa_handler = handler;
    sa.sa_flags = SA_RESETHAND;  // Handler resets after first invocation
    sigemptyset(&sa.sa_mask);

    sigaction(SIGINT, &sa, NULL);
    
    if (fork() == 0) {
        printf("child send SIGINT\n");
        kill(getppid(), SIGINT);
        sleep(1);
        printf("child send SIGINT again\n");
        kill(getppid(), SIGINT);
        exit(0);
    }

    while (1) {
    }

    return 0;
}
