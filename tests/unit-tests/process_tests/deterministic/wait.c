#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>

int main(void) {
    pid_t pid = fork();

    if (pid == 0) {
        // Child
        sleep(1);
        return 0;
    } else {
        // Parent
        wait(NULL);
        printf("Parent detected child finished.\n");
    }

    pid = fork();

    if (pid == 0) {
        // Child
        sleep(1);
    } else {
        // Parent
        int status = -1;
        while(wait(&status) == -1);
        printf("Child exited with status %d\n", status);
    }

    return 0;
}
