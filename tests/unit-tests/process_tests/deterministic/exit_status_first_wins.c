/*
 * Test: exit_status_first_wins — exit_syscall (SYS_exit) must not record
 * the cage exit status; only exit_group_syscall should.
 *
 * Bug:
 *   exit_syscall() called cage_record_exit_status(0) even though SYS_exit
 *   is a thread-only exit (triggered by glibc start_thread when a non-main
 *   pthread returns).  cage_record_exit_status uses first-write-wins
 *   semantics, so if SYS_exit(0) fires before exit_group_syscall(1), the
 *   cage's final_exit_status is locked at 0 and exit_group(1) cannot
 *   overwrite it.  cage_finalize then reports exit code 0 to the parent.
 *
 * Scenario that reliably triggers the bug:
 *   1. Child forks.
 *   2. Child creates thread A that immediately returns NULL.  glibc
 *      start_thread does, in order:
 *        atomic_store_release(&pd->tid, 0)  -- marks thread done
 *        FUTEX_WAKE(&pd->tid, 1)            -- unblocks pthread_join
 *        EXIT_SYSCALL(0)                    -- very next instruction
 *      On the unfixed branch, exit_syscall calls cage_record_exit_status(0)
 *      at the start of exit_syscall, before asyncify unwind.
 *   3. pthread_join(ta) in the child main thread returns right after
 *      FUTEX_WAKE, so thread A is still about to call EXIT_SYSCALL(0).
 *      Main's path from pthread_join back to exit(1) requires asyncify
 *      rewind and several library frames; thread A's
 *      cage_record_exit_status(0) runs first and locks final_exit_status=0.
 *   4. Child main calls exit(1).  exit_group_syscall tries to record 1
 *      but final_exit_status is already Some(0) — no overwrite.
 *
 * Before fix: cage_finalize reads Some(0) → parent waitpid sees 0 (WRONG).
 * After fix:  exit_syscall does not record status → exit_group(1) is the
 *             first write → cage_finalize reads Some(1) → parent sees 1.
 */

#include <assert.h>
#include <pthread.h>
#include <stdlib.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

static void *thread_a(void *arg)
{
    (void)arg;
    /*
     * Return immediately.  glibc start_thread will call FUTEX_WAKE then
     * EXIT_SYSCALL(0).  On the unfixed branch, exit_syscall records
     * final_exit_status = Some(0) before main can call exit(1).
     */
    return NULL;
}

int main(void)
{
    pid_t pid = fork();
    assert(pid != -1 && "fork should succeed");

    if (pid == 0) {
        pthread_t ta;
        int rc = pthread_create(&ta, NULL, thread_a, NULL);
        assert(rc == 0 && "pthread_create should succeed");

        /*
         * pthread_join returns after FUTEX_WAKE but before EXIT_SYSCALL.
         * Thread A will call EXIT_SYSCALL on its next instruction.
         * Main's return path from pthread_join to exit(1) goes through
         * asyncify rewind + multiple library frames, so on the unfixed
         * branch thread A's cage_record_exit_status(0) reliably fires
         * first, locking final_exit_status = Some(0).
         *
         * On the fixed branch exit_syscall does not call
         * cage_record_exit_status, so exit_group(1) below records Some(1).
         */
        pthread_join(ta, NULL);
        exit(1);

        _exit(99); /* unreachable */
    }

    int status;
    pid_t waited = waitpid(pid, &status, 0);
    assert(waited == pid && "waitpid should return child pid");
    assert(WIFEXITED(status) && "child should exit normally");
    assert(WEXITSTATUS(status) == 1 &&
           "exit_group(1) should win over thread SYS_exit(0)");

    printf("exit_status_first_wins: PASS\n");
    return 0;
}
