/*
 * Test: exec_crash_finalize — parent's waitpid() must not hang when an
 * exec'd child crashes.
 *
 * Scenario:
 *   1. Parent forks a child.
 *   2. Child execs exec_crash_helper, which calls abort() and triggers a
 *      wasm trap immediately.
 *   3. Before the fix, cage_finalize() was never called for the exec'd
 *      module, so parent's waitpid() would block forever.
 *   4. After the fix, cage_finalize() is called with exit code 1 on any
 *      wasm trap, mirroring the fork-crash cleanup path.
 *
 * NOTE: Before running, compile exec_crash_helper.c and place the binary at
 *       $LIND_FS_ROOT/automated_tests/exec_crash_helper.
 */

#include <assert.h>
#include <stdio.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
    pid_t pid = fork();
    assert(pid != -1 && "fork should succeed");

    if (pid == 0) {
        char *argv[] = {"exec_crash_helper.cwasm", NULL};
        execv("automated_tests/exec_crash_helper", argv);
        /* execv should not return */
        _exit(99);
    }

    /* Parent: waitpid must return (not hang) */
    int status;
    pid_t waited = waitpid(pid, &status, 0);
    assert(waited == pid && "waitpid should return child pid");

    printf("status = %d\n", status);

    /*
     * The crash finalize path calls cage_finalize with exit code 1,
     * so the child should exit with status 1.
     */
    assert(WIFEXITED(status) && "child should have exited");
    assert(WEXITSTATUS(status) == 1 && "crash should produce exit code 1");

    printf("exec_crash_finalize: PASS\n");
    return 0;
}
