#include <assert.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    sigset_t mask;
    assert(sigemptyset(&mask) == 0);
    assert(sigaddset(&mask, SIGUSR1) == 0);
    assert(sigprocmask(SIG_BLOCK, &mask, NULL) == 0);

    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        sigset_t child_mask;
        if (sigprocmask(SIG_BLOCK, NULL, &child_mask) != 0 ||
            sigismember(&child_mask, SIGUSR1) != 1) {
            _exit(1);
        }
        _exit(0);
    }

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    return 0;
}
