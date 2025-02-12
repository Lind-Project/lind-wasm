// test for terminating all threads when the process is killed by signal

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <pthread.h>

void* thread_function(void* arg) {
    printf("Hello from thread\n");
    for(;;)
    {
        printf("thread sleep\n");
        sleep(1);
    }
    return NULL;
}

int main() {
    printf("main starts!\n");

    pthread_t thread;
    pthread_create(&thread, NULL, thread_function, NULL);

    sleep(1);

    int pid = fork();
    if(pid == 0)
    {
        printf("child ready to kill\n");
        kill(getppid(), SIGINT);
        printf("child done kill\n");
        while(1) {
            printf("child in loop, pid=%d\n", getpid());
            sleep(1);
        }
    }
    else
    {
        while (1) {
            // wait for signals
        }
        printf("parent outside loop (should not reach)\n");
    }

    return 0;
}
