/*
 * fork_after_exec_outer.c — The "outer" binary that forks and execs the inner.
 *
 * Flow:
 *   outer: fork → child execs "automated_tests/fork_after_exec_inner"
 *                 → inner: forks again → child prints → parent waits → done
 *
 * This tests: fork → exec → exec'd binary forks internally.
 * This is the exact pattern that hangs with lmbench benchmarks.
 *
 * Before running:
 *   The test harness auto-detects "automated_tests/fork_after_exec_inner"
 *   and copies it into lindfs/automated_tests/
 *
 * Expected output:
 *   outer: forking
 *   outer: parent waiting for child
 *   inner: parent before fork
 *   inner: fork returned child_pid=<N>
 *   inner: child running, pid=<M>
 *   inner: child exiting
 *   inner: waitpid returned
 *   inner: done
 *   outer: child exited with status 0
 *   outer: done
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

int main() {
    fprintf(stderr, "outer: forking\n");

    pid_t pid = fork();
    if (pid < 0) {
        perror("outer: fork failed");
        return 1;
    }

    if (pid == 0) {
        /* child: exec the inner binary */
        char *argv[] = {"fork_after_exec_inner", NULL};
        execv("automated_tests/fork_after_exec_inner", argv);
        perror("outer: execv failed");
        _exit(1);
    }

    /* parent */
    fprintf(stderr, "outer: parent waiting for child\n");

    int status;
    pid_t w = waitpid(pid, &status, 0);
    if (w < 0) {
        perror("outer: waitpid failed");
        return 1;
    }

    if (WIFEXITED(status)) {
        fprintf(stderr, "outer: child exited with status %d\n", WEXITSTATUS(status));
    } else {
        fprintf(stderr, "outer: child terminated abnormally\n");
    }

    fprintf(stderr, "outer: done\n");
    return 0;
}
