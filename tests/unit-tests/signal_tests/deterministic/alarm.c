// test for alarm function

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

void alarm_handler(int sig) {
    printf("Alarm triggered! Signal received: %d\n", sig);

    // Manually reset the alarm for periodic execution (every 3 seconds)
    alarm(3);
}

int main() {
    // Set up the signal handler for SIGALRM
    struct sigaction sa;
    sa.sa_handler = alarm_handler;
    sa.sa_flags = 0;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGALRM, &sa, NULL);

    printf("Setting an alarm to trigger in 1 seconds...\n");
    alarm(1);  // First alarm triggers after 2 seconds

    while (1) {
        // Wait for signals
    }

    return 0;
}
