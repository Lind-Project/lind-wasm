// test for sigprocmask syscall

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/types.h>

void check_signal_status(int signum) {
    sigset_t current_mask;
    sigprocmask(SIG_BLOCK, NULL, &current_mask);  // Get current signal mask

    if (sigismember(&current_mask, signum)) {
        printf("Signal %d is BLOCKED\n", signum);
    } else {
        printf("Signal %d is UNBLOCKED\n", signum);
    }
}

void sigint_handler(int sig) {
    printf("SIGINT received! (Handled in Parent Process)\n");
}

int main() {
    struct sigaction sa;
    sa.sa_handler = sigint_handler;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGINT, &sa, NULL);  // Set handler for SIGINT

    sigset_t block_set;
    sigemptyset(&block_set);
    sigaddset(&block_set, SIGINT);

    printf("Parent: Blocking SIGINT...\n");
    sigprocmask(SIG_BLOCK, &block_set, NULL);  // Block SIGINT
    check_signal_status(SIGINT);

    pid_t pid = fork();

    if (pid < 0) {
        perror("Fork failed");
        exit(1);
    }

    if (pid == 0) {  // Child Process
        // sleep(3);  // Give parent some time before sending the signal
        printf("Child: Sending SIGINT to parent (PID: %d)\n", getppid());
        kill(getppid(), SIGINT);
        exit(0);
    } else {  // Parent Process
        printf("Parent: SIGINT is blocked. Child will send SIGINT soon...\n");
        int tmp = 2;
        while(tmp--)
        {
            sleep(1);  // Allow time for the signal to be sent
        }

        printf("Parent: Unblocking SIGINT now.\n");
        sigprocmask(SIG_UNBLOCK, &block_set, NULL);  // Unblock SIGINT
        check_signal_status(SIGINT);

        printf("Parent: Waiting for SIGINT...\n");
        while (1) {
            // pause();  // Wait for signals
        }
    }

    return 0;
}
