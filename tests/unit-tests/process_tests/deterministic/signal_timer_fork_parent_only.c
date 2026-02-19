/*
 * signal_timer_fork_parent_only.c
 *
 * Fork first, set up signal+timer in PARENT only.
 * Child exits immediately.
 *
 * Tests: does signal delivery work in parent after forking?
 *
 * Expected output:
 *   parent: forked child pid=N
 *   child: exiting immediately
 *   parent: installing SIGALRM handler
 *   parent: setting itimer (200ms)
 *   parent: looping...
 *   parent: SIGALRM caught!
 *   parent: exiting after alarm
 */

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <sys/time.h>
#include <sys/wait.h>

static volatile int alarm_fired = 0;

void alarm_handler(int signo) {
    fprintf(stderr, "parent: SIGALRM caught!\n");
    alarm_fired = 1;
}

int main() {
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        return 1;
    }

    if (pid == 0) {
        /* child: exit immediately */
        fprintf(stderr, "child: exiting immediately\n");
        _exit(0);
    }

    /* parent: wait for child first, then set up signal */
    fprintf(stderr, "parent: forked child pid=%d\n", pid);
    waitpid(pid, NULL, 0);

    fprintf(stderr, "parent: installing SIGALRM handler\n");
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGALRM, &sa, NULL);

    fprintf(stderr, "parent: setting itimer (200ms)\n");
    struct itimerval it;
    it.it_value.tv_sec = 0;
    it.it_value.tv_usec = 200000;
    it.it_interval.tv_sec = 0;
    it.it_interval.tv_usec = 0;
    setitimer(ITIMER_REAL, &it, NULL);

    fprintf(stderr, "parent: looping...\n");
    while (!alarm_fired) {
        /* spin */
    }
    fprintf(stderr, "parent: exiting after alarm\n");
    return 0;
}
