/*
 * fork_after_exec_inner_signal.c — Inner binary with signal + timer.
 *
 * Mimics benchmp's core pattern: installs SIGALRM handler, sets
 * itimer, forks, child does work until timer fires.
 *
 * This tests whether signal delivery works correctly inside a
 * binary that was reached via exec.
 *
 * Expected output:
 *   inner_sig: installing SIGALRM handler
 *   inner_sig: setting itimer (200ms)
 *   inner_sig: forking
 *   inner_sig: child looping...
 *   inner_sig: SIGALRM caught!
 *   inner_sig: child exiting after alarm
 *   inner_sig: waitpid returned
 *   inner_sig: done
 */

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <sys/time.h>
#include <sys/wait.h>

static volatile int alarm_fired = 0;

void alarm_handler(int signo) {
    fprintf(stderr, "inner_sig: SIGALRM caught!\n");
    alarm_fired = 1;
}

int main() {
    fprintf(stderr, "inner_sig: installing SIGALRM handler\n");
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    if (sigaction(SIGALRM, &sa, NULL) < 0) {
        perror("inner_sig: sigaction failed");
        return 1;
    }

    fprintf(stderr, "inner_sig: setting itimer (200ms)\n");
    struct itimerval it;
    it.it_value.tv_sec = 0;
    it.it_value.tv_usec = 200000; /* 200ms */
    it.it_interval.tv_sec = 0;
    it.it_interval.tv_usec = 0;
    if (setitimer(ITIMER_REAL, &it, NULL) < 0) {
        perror("inner_sig: setitimer failed");
        return 1;
    }

    fprintf(stderr, "inner_sig: forking\n");
    pid_t pid = fork();
    if (pid < 0) {
        perror("inner_sig: fork failed");
        return 1;
    }

    if (pid == 0) {
        /* child: loop until alarm fires */
        fprintf(stderr, "inner_sig: child looping...\n");
        while (!alarm_fired) {
            /* spin — alarm_handler sets alarm_fired */
        }
        fprintf(stderr, "inner_sig: child exiting after alarm\n");
        _exit(0);
    }

    /* parent: also wait for alarm, then reap child */
    while (!alarm_fired) {
        /* spin */
    }

    int status;
    waitpid(pid, &status, 0);
    fprintf(stderr, "inner_sig: waitpid returned\n");
    fprintf(stderr, "inner_sig: done\n");
    return 0;
}
