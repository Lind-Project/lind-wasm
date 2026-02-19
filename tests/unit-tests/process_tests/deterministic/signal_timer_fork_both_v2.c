/*
 * signal_timer_fork_both_v2.c
 *
 * Install handler before fork, but set timer AFTER fork
 * in both parent and child independently.
 *
 * Expected: both should get SIGALRM and exit cleanly.
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
    fprintf(stderr, "installing SIGALRM handler\n");
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGALRM, &sa, NULL);

    fprintf(stderr, "forking\n");
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        return 1;
    }

    /* Both parent and child set their OWN timer after fork */
    struct itimerval it;
    it.it_value.tv_sec = 0;
    it.it_value.tv_usec = 200000;
    it.it_interval.tv_sec = 0;
    it.it_interval.tv_usec = 0;
    setitimer(ITIMER_REAL, &it, NULL);

    if (pid == 0) {
        fprintf(stderr, "child: timer set, looping... pid=%d\n", getpid());
        while (!alarm_fired) { }
        fprintf(stderr, "child: exiting after alarm\n");
        _exit(0);
    }

    fprintf(stderr, "parent: timer set, looping... pid=%d\n", getpid());
    while (!alarm_fired) { }
    fprintf(stderr, "parent: alarm done, waiting for child\n");

    int status;
    waitpid(pid, &status, 0);
    fprintf(stderr, "parent: waitpid returned\n");
    fprintf(stderr, "parent: done\n");
    return 0;
}
