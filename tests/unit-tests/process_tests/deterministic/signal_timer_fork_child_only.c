/*
 * signal_timer_fork_child_only.c
 *
 * Fork first, THEN set up signal+timer in child only.
 * Parent just waits.
 *
 * Tests: does signal delivery work in a forked child?
 *
 * Expected output:
 *   parent: forked child pid=N
 *   child: installing SIGALRM handler
 *   child: setting itimer (200ms)
 *   child: looping...
 *   child: SIGALRM caught!
 *   child: exiting after alarm
 *   parent: waitpid returned
 *   parent: done
 */

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <sys/time.h>
#include <sys/wait.h>

static volatile int alarm_fired = 0;

void alarm_handler(int signo) {
    fprintf(stderr, "child: SIGALRM caught!\n");
    alarm_fired = 1;
}

int main() {
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        return 1;
    }

    if (pid == 0) {
        /* child: set up signal + timer here */
        fprintf(stderr, "child: installing SIGALRM handler\n");
        struct sigaction sa;
        sa.sa_handler = alarm_handler;
        sigemptyset(&sa.sa_mask);
        sa.sa_flags = 0;
        sigaction(SIGALRM, &sa, NULL);

        fprintf(stderr, "child: setting itimer (200ms)\n");
        struct itimerval it;
        it.it_value.tv_sec = 0;
        it.it_value.tv_usec = 200000;
        it.it_interval.tv_sec = 0;
        it.it_interval.tv_usec = 0;
        setitimer(ITIMER_REAL, &it, NULL);

        fprintf(stderr, "child: looping...\n");
        while (!alarm_fired) {
            /* spin */
        }
        fprintf(stderr, "child: exiting after alarm\n");
        _exit(0);
    }

    /* parent: just wait */
    fprintf(stderr, "parent: forked child pid=%d\n", pid);
    int status;
    waitpid(pid, &status, 0);
    fprintf(stderr, "parent: waitpid returned\n");
    fprintf(stderr, "parent: done\n");
    return 0;
}
