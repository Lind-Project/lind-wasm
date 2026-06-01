/*
 * Test: signal-terminated process properly cleans up so parent can waitpid.
 *
 * This exercises the asyncify-based exit path for signal termination:
 *   1. Parent forks a child
 *   2. Child spins in wasm (allowing epoch to fire)
 *   3. Parent sends SIGTERM to child (default handler = Terminate)
 *   4. Child's epoch callback delivers SIGTERM, triggers cage_exit_cleanup +
 *      asyncify unwind instead of the old thread_suicide trap
 *   5. Parent's waitpid returns with WIFSIGNALED, WTERMSIG == SIGTERM
 *
 * Before the fix, thread_suicide() raised Trap::Interrupt which bypassed
 * all exit cleanup (no zombie, no SIGCHLD, no lind_manager.decrement),
 * causing waitpid to block forever and lind-boot to hang.
 */

#include <assert.h>
#include <signal.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void)
{
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        /* child: spin in wasm so epoch can deliver signals */
        while (1)
            ;
        _exit(99); /* unreachable */
    }

    /* parent: give child time to start spinning */
    sleep(1);

    /* send SIGTERM — default handler is Terminate */
    int ret = kill(pid, SIGTERM);
    assert(ret == 0);

    /* waitpid should return because the child created a zombie via
     * cage_exit_cleanup (not thread_suicide which skipped cleanup) */
    int status = 0;
    pid_t waited = waitpid(pid, &status, 0);
    assert(waited == pid);

    assert(WIFSIGNALED(status));
    assert(WTERMSIG(status) == SIGTERM);

    return 0;
}
