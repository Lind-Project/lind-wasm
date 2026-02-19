/*
 * signal_timer_nofork.c — SIGALRM + itimer WITHOUT fork.
 *
 * Tests whether signal delivery works in a single process
 * (no fork involved). If this passes, the bug is in the
 * interaction between fork and signal delivery.
 *
 * Expected output:
 *   nofork: installing SIGALRM handler
 *   nofork: setting itimer (200ms)
 *   nofork: looping...
 *   nofork: SIGALRM caught!
 *   nofork: exiting after alarm
 */

#include <stdio.h>
#include <signal.h>
#include <sys/time.h>

static volatile int alarm_fired = 0;

void alarm_handler(int signo) {
    fprintf(stderr, "nofork: SIGALRM caught!\n");
    alarm_fired = 1;
}

int main() {
    fprintf(stderr, "nofork: installing SIGALRM handler\n");
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    sigaction(SIGALRM, &sa, NULL);

    fprintf(stderr, "nofork: setting itimer (200ms)\n");
    struct itimerval it;
    it.it_value.tv_sec = 0;
    it.it_value.tv_usec = 200000;
    it.it_interval.tv_sec = 0;
    it.it_interval.tv_usec = 0;
    setitimer(ITIMER_REAL, &it, NULL);

    fprintf(stderr, "nofork: looping...\n");
    while (!alarm_fired) {
        /* spin */
    }

    fprintf(stderr, "nofork: exiting after alarm\n");
    return 0;
}
