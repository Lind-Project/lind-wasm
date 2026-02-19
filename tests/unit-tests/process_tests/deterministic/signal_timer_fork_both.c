/*
 * signal_timer_fork_both.c
 *
 * Set up signal+timer BEFORE fork, so both parent and child
 * inherit the handler and timer. Both spin until alarm fires.
 *
 * This is the pattern that hangs. Isolates the issue to:
 * "two cages with active timers/signals simultaneously"
 *
 * Expected output (if working):
 *   both: installing SIGALRM handler
 *   both: setting itimer (200ms)
 *   both: forking
 *   parent: looping... pid=1
 *   child: looping... pid=2
 *   (one of): SIGALRM caught in pid=N
 *   (one of): SIGALRM caught in pid=M
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
    fprintf(stderr, "SIGALRM caught in pid=%d\n", getpid());
    alarm_fired = 1;
}

int main() {
    fprintf(stderr, "both: installing SIGALRM handler\n");
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGALRM, &sa, NULL);

    fprintf(stderr, "both: setting itimer (200ms)\n");
    struct itimerval it;
    it.it_value.tv_sec = 0;
    it.it_value.tv_usec = 200000;
    it.it_interval.tv_sec = 0;
    it.it_interval.tv_usec = 0;
    setitimer(ITIMER_REAL, &it, NULL);

    fprintf(stderr, "both: forking\n");
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        return 1;
    }

    if (pid == 0) {
        fprintf(stderr, "child: looping... pid=%d\n", getpid());
        while (!alarm_fired) { }
        fprintf(stderr, "child: exiting after alarm\n");
        _exit(0);
    }

    fprintf(stderr, "parent: looping... pid=%d\n", getpid());
    while (!alarm_fired) { }
    fprintf(stderr, "parent: alarm done, waiting for child\n");

    int status;
    waitpid(pid, &status, 0);
    fprintf(stderr, "parent: waitpid returned\n");
    fprintf(stderr, "parent: done\n");
    return 0;
}
