// signal test for SIGCHLD signal when child exits

#include <assert.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>

volatile sig_atomic_t got_sigchld = 0;

// Custom signal handler - async-signal-safe
void handle_sigchld(int signal) {
    if (signal == SIGCHLD) {
        got_sigchld = 1;
    }
}

int main() {
    struct sigaction sa;
    sa.sa_handler = handle_sigchld;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);

    assert(sigaction(SIGCHLD, &sa, NULL) == 0);

    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        // Child process
        _exit(0);
    } else {
        // Parent process
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);

        // Check if SIGCHLD was received (may need brief bounded loop)
        for (int i = 0; i < 1000000 && !got_sigchld; i++) {
            // Busy wait - no sleep
        }
        assert(got_sigchld == 1);
    }

    return 0;
}
