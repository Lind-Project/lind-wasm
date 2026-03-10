#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <signal.h>
#include <sys/wait.h>

int main(void) {
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        return 1;
    }

    if (pid == 0) {
        // child: wait for parent to kill us
        while (1) {
            
        }
        _exit(123);
    }

    sleep(1);

    if (kill(pid, SIGKILL) != 0) {
        perror("kill(SIGKILL)");
        return 1;
    }

    int status = 0;
    if (waitpid(pid, &status, 0) < 0) {
        perror("waitpid");
        return 1;
    }

    printf("raw wait status = 0x%x\n", status);

    if (!WIFSIGNALED(status)) {
        printf("FAIL: child not reported as signaled\n");
        return 2;
    }

    int sig = WTERMSIG(status);
    printf("child terminated by signal %d\n", sig);

    if (sig != SIGKILL) {
        printf("FAIL: expected SIGKILL (%d), got %d\n", SIGKILL, sig);
        return 3;
    }

    printf("PASS\n");
    return 0;
}
