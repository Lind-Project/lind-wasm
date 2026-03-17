#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    pid_t pid = fork();

    if (pid == 0) {
        // Child process: exit with a non-zero code to indicate failure
        exit(EXIT_FAILURE);
    }

    int status;
    wait(&status);

    if (WIFEXITED(status)) {
        int code = WEXITSTATUS(status);
        if (code != 0) {
            printf("Child exited with non-zero as expected (%d)\n", code);
            return 0;  
        } else {
            printf("Child exited with zero (unexpected)\n");
            return EXIT_FAILURE;
        }
    }

    printf("Child did not exit normally\n");
    return EXIT_FAILURE;
}
