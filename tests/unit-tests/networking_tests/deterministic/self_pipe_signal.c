/*
 * Self-pipe signal-wakeup test (postgres WaitLatch pattern).
 *
 * The "self-pipe trick": process owns a pipe; its signal handler write()s
 * a byte to the pipe's write end; its main loop poll()s the pipe's read
 * end.  poll() must wake when the signal handler writes, even though the
 * write happens from inside the handler invoked via signal delivery.
 *
 * This is exactly what PostgreSQL's WaitLatchOrSocket / SetLatch do: a
 * sibling/parent kills the process with SIGUSR1, the SIGUSR1 handler
 * writes 1 byte to the self-pipe, the main loop's poll() returns POLLIN
 * and the loop runs the queued work (e.g., processing a ProcSignalBarrier).
 *
 * Failure modes this catches:
 *   - poll() on a pipe doesn't wake when a write hits the other end.
 *   - signal-handler-issued write() is silently dropped.
 *   - poll() blocks indefinitely despite the pipe having data.
 */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int self_pipe_w = -1;

static void wakeup_handler(int signo) {
    (void)signo;
    char b = 'x';
    /* Best-effort write — ignore EAGAIN from a full pipe (one-byte signal). */
    (void)write(self_pipe_w, &b, 1);
}

int main(void) {
    int p[2];
    if (pipe(p) < 0) { perror("pipe"); return 1; }

    /* Make the read end blocking but leave write end normal. */
    int flags_r = fcntl(p[0], F_GETFL, 0);
    if (flags_r < 0) { perror("fcntl GETFL"); return 1; }

    self_pipe_w = p[1];

    struct sigaction sa = {0};
    sa.sa_handler = wakeup_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_RESTART;
    if (sigaction(SIGUSR1, &sa, NULL) < 0) {
        perror("sigaction"); return 1;
    }

    /* Share parent_pid via shmem so child can kill us. */
    void *region = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                        MAP_ANONYMOUS | MAP_SHARED, -1, 0);
    if (region == MAP_FAILED) { perror("mmap"); return 1; }
    volatile pid_t *parent_pid_cell = (volatile pid_t *)region;
    *parent_pid_cell = getpid();

    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }
    if (pid == 0) {
        /* Child: small delay so parent's poll() is actually entered, then
         * kill the parent.  Closing pipe ends in the child to avoid
         * keeping write_refs > 0 on the parent's pipe inappropriately. */
        close(p[0]);
        close(p[1]);
        struct timespec ts = { .tv_sec = 0, .tv_nsec = 200 * 1000 * 1000 };
        nanosleep(&ts, NULL);
        kill(*parent_pid_cell, SIGUSR1);
        _exit(0);
    }

    /* Parent: poll the read end with a 5s timeout. */
    struct pollfd pfd = { .fd = p[0], .events = POLLIN };
    int n = poll(&pfd, 1, 5000);
    if (n == 0) {
        fprintf(stderr,
                "[parent] FAIL: poll timed out — signal-handler write to "
                "self-pipe didn't wake poll()\n");
        return 1;
    }
    if (n < 0) {
        if (errno == EINTR) {
            fprintf(stderr,
                    "[parent] FAIL: poll returned EINTR — signal interrupted "
                    "poll() but didn't (or before) write to self-pipe\n");
        } else {
            perror("[parent] poll");
        }
        return 1;
    }
    if (!(pfd.revents & POLLIN)) {
        fprintf(stderr, "[parent] FAIL: poll returned %d but revents=0x%x\n",
                n, pfd.revents);
        return 1;
    }

    char b;
    if (read(p[0], &b, 1) != 1) {
        perror("[parent] read");
        return 1;
    }

    int status = 0;
    waitpid(pid, &status, 0);
    fprintf(stderr, "[parent] PASS\n");
    munmap(region, 4096);
    close(p[0]);
    close(p[1]);
    return 0;
}
