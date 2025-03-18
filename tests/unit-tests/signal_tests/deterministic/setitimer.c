// test for setitimer syscall

#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <sys/time.h>
#include <unistd.h>

int signal_counter = 3;

void alarm_handler(int sig) {
    signal_counter -= 1;
    printf("Timer expired! Signal received: %d\n", sig);
}

int main() {
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGALRM, &sa, NULL);  // Handle SIGALRM

    struct itimerval timer;
    
    // First expiration after 1 seconds
    timer.it_value.tv_sec = 1;
    timer.it_value.tv_usec = 0;

    // Interval for periodic execution (every 3 seconds)
    timer.it_interval.tv_sec = 3;
    timer.it_interval.tv_usec = 0;

    // Set the timer (ITIMER_REAL sends SIGALRM)
    setitimer(ITIMER_REAL, &timer, NULL);

    printf("Timer started! SIGALRM will fire every 3 seconds.\n");

    while (signal_counter > 0) {
        // Wait for signals
    }

    return 0;
}
