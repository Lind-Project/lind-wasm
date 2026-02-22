#include <signal.h>
#include <unistd.h>

static volatile sig_atomic_t in_usr1 = 0;
static volatile sig_atomic_t done_usr1 = 0;
static volatile sig_atomic_t got_usr2 = 0;
static volatile sig_atomic_t saw_usr2_during_usr1 = 0;

static void handler_usr1(int sig) {
    (void)sig;
    in_usr1 = 1;
    kill(getpid(), SIGUSR2);
    if (got_usr2)
        saw_usr2_during_usr1 = 1;
    done_usr1 = 1;
}

static void handler_usr2(int sig) {
    (void)sig;
    got_usr2 = 1;
}

int main(void) {
    struct sigaction sa;

    sa.sa_handler = handler_usr1;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    sigaddset(&sa.sa_mask, SIGUSR2);
    if (sigaction(SIGUSR1, &sa, NULL) != 0)
        return 1;

    sa.sa_handler = handler_usr2;
    sigemptyset(&sa.sa_mask);
    if (sigaction(SIGUSR2, &sa, NULL) != 0)
        return 1;

    kill(getpid(), SIGUSR1);
    while (!done_usr1)
        ;

    if (done_usr1 != 1 || got_usr2 != 1 || saw_usr2_during_usr1 != 0)
        return 1;
    return 0;
}
