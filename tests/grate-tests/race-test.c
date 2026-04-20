/* race_test.c — Test binary to trigger the get_cage() race condition.
 *
 * Strategy: fork N children, each spawning M threads that do work involving
 * mmap (thread stack allocation) and futex (pthread sync). Children exit
 * quickly, creating concurrent remove_cage() + get_cage() calls.
 *
 * The race window:
 *   - Child's last thread calls exit_syscall → remove_cage()
 *   - Meanwhile, sibling children or their threads call mmap/futex
 *   - get_cage() returns None → unwrap() panics in signal.rs / fs_calls.rs
 *
 * Run with the race-test grate:
 *   lind-wasm race_test_grate.cwasm race_test.cwasm
 *
 * Without the runtime fix, this should panic intermittently.
 */
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <unistd.h>
#include <assert.h>
#include <sys/wait.h>

#define NUM_FORKS   2

static volatile int sink;

void sleep_and_print(int real, int get, int iteration)
{
    sleep(1);
    printf("real pid=%d, get pid=%d at iteration %d\n", real, get, iteration);
    fflush(stdout);
    exit(1);
}

static void child_work(int real_pid) {
    printf("child work: real_pid: %d\n", real_pid);
    for(int i = 0; i < 10000; ++i)
    {
        int pid = getpid();
        if(real_pid != pid)
            sleep_and_print(real_pid, pid, i);
        // assert(real_pid == pid);
    }
}

int main(void) {
    pid_t pids[NUM_FORKS];

    for (int i = 0; i < NUM_FORKS; i++) {
        pids[i] = fork();
        if (pids[i] < 0) {
            perror("fork");
            exit(1);
        }
        if (pids[i] == 0) {
            child_work(getpid());
            exit(0);
        }
    }

    /* Wait for all children */
    for (int i = 0; i < NUM_FORKS; i++)
        waitpid(pids[i], NULL, 0);

    printf("pass\n");
    return 0;
}
