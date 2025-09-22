#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <errno.h>

/* Runtime expects this export when signals (e.g., SIGCHLD) may fire */
__attribute__((export_name("signal_callback")))
void signal_callback(int signo, int aux) { (void)signo; (void)aux; }

/*
 * Test waitpid with WNOHANG flag
 * Returns 0 for success, 1 for failure
 */
int main(void)
{
    pid_t cpid, wpid;
    int status = 0;

    cpid = fork();
    if (cpid == -1) {
        return 1;  // Fork failed
    }

    if (cpid == 0) {
        /* Child process - do some work and exit with known status */
        volatile int i;
        for (i = 0; i < 1000000; i++) { /* burn cycles */ }
        exit(42);  /* Exit with specific code for testing */
    } else {
        /* Parent process */

        /* Test 1: Call waitpid with WNOHANG */
        wpid = waitpid(cpid, &status, WNOHANG);
        if (wpid == 0) {
            /* Child not ready, now wait for child to complete (blocking) */
            wpid = waitpid(cpid, &status, 0);
        } else if (wpid != cpid) {
            return 1;  // waitpid failed
        }

        /* Verify child was reaped */
        if (wpid != cpid) {
            return 1;  // Child reaping failed
        }

        /* Check exit status (support POSIX and raw Wasm formats) */
        int exit_code_ok = 0;
        if (WIFEXITED(status) && WEXITSTATUS(status) == 42) {
            exit_code_ok = 1;  // POSIX format
        } else if (status == 42) {
            exit_code_ok = 1;  // Raw exit code
        }
        if (!exit_code_ok) {
            return 1;  // Wrong exit code
        }

        /* Test 2: Call waitpid again - should return -1 with ECHILD */
        errno = 0;  // guard against stale errno
        wpid = waitpid(cpid, &status, WNOHANG);
        if (wpid != -1 || errno != ECHILD) {
            return 1;  // Second waitpid should fail with ECHILD
        }
    }

    return 0;  // success
}

