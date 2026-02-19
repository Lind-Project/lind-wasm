/*
 * fork_after_exec_inner.c — The "inner" binary that gets exec'd.
 *
 * This program forks, prints from both parent and child, then waits.
 * If exec + subsequent fork works, you'll see output from both.
 * If it hangs, the fork-after-exec bug is confirmed at the runtime level.
 *
 * Used by: fork_after_exec_outer.c (which fork+execs this binary)
 * Also runnable standalone to confirm it works without exec.
 *
 * Expected output (standalone):
 *   inner: parent before fork
 *   inner: fork returned child_pid=<N>
 *   inner: child running, pid=<M>
 *   inner: child exiting
 *   inner: waitpid returned
 *   inner: done
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

int main() {
    fprintf(stderr, "inner: parent before fork\n");

    pid_t pid = fork();
    if (pid < 0) {
        perror("inner: fork failed");
        return 1;
    }

    if (pid == 0) {
        /* child */
        fprintf(stderr, "inner: child running, pid=%d\n", getpid());
        fprintf(stderr, "inner: child exiting\n");
        _exit(0);
    }

    /* parent */
    fprintf(stderr, "inner: fork returned child_pid=%d\n", pid);

    int status;
    pid_t w = waitpid(pid, &status, 0);
    if (w < 0) {
        perror("inner: waitpid failed");
        return 1;
    }
    fprintf(stderr, "inner: waitpid returned\n");
    fprintf(stderr, "inner: done\n");

    return 0;
}
