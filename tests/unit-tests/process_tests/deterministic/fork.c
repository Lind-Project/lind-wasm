#include <stdio.h>
#include <unistd.h>
#include <sys/types.h>

int main() {
    pid_t pid = fork();

    if (pid < 0) {
        perror("fork failed");
        return 1;
    } else if (pid == 0) {
        printf("This is the child process. PID: %d\n", getpid());
    } else {
        printf("This is the parent process. Child PID: %d, My PID: %d\n", pid, getpid());
    }

    return 0;
}
