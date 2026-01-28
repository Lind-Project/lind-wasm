#include <assert.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>

int main() {
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        exit(0);
    } else {
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    return 0;
}
