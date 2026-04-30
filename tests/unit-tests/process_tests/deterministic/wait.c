#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
#include <assert.h>

int main(void) {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        return 0;
    } else {
        int ret = wait(NULL);
        assert(ret >= 0);
    }

    pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        return 0;
    } else {
        int status = -1;
        while (wait(&status) == -1)
            ;
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    printf("wait: all tests passed\n");
    return 0;
}
