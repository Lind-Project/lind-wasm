#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <errno.h>

/*
 * Test waitpid with WNOHANG flag
 * Returns 0 for success, 1 for failure
 */
int main()
{
    pid_t cpid, wpid;
    int status;
    
    cpid = fork();
    if (cpid == -1) {
        return 1;  // Fork failed
    }
    
    if (cpid == 0) {
        /* Child process - do some work and exit with known status */
        volatile int i;
        for (i = 0; i < 1000000; i++) {
            /* Just burn some cycles */
        }
        exit(42);  /* Exit with specific code for testing */
    } else {
        /* Parent process */
        
        /* Test 1: Call waitpid with WNOHANG */
        wpid = waitpid(cpid, &status, WNOHANG);
        
        if (wpid == 0) {
            /* Child not ready, now wait for child to complete */
            wpid = waitpid(cpid, &status, 0);  /* Blocking wait */
        } else if (wpid != cpid) {
            return 1;  // waitpid failed
        }
        
        /* Verify child was reaped - handle both POSIX and WebAssembly status formats */
        if (wpid != cpid) {
            return 1;  // Child reaping failed
        }
        
        // Check exit status (handle both POSIX and raw formats)
        int exit_code_ok = 0;
        if (WIFEXITED(status) && WEXITSTATUS(status) == 42) {
            exit_code_ok = 1;  // Native/correct POSIX format
        } else if (status == 42) {
            exit_code_ok = 1;  // WebAssembly format - status is raw exit code
        }
        
        if (!exit_code_ok) {
            return 1;  // Wrong exit code
        }
        
        /* Test 2: Call waitpid again - should return -1 with ECHILD */
        wpid = waitpid(cpid, &status, WNOHANG);
        if (wpid != -1 || errno != ECHILD) {
            return 1;  // Second waitpid should fail with ECHILD
        }
        
        /* Test 3: Test with invalid PID */
        wpid = waitpid(99999, &status, WNOHANG);
        if (wpid != -1 || errno != ECHILD) {
            return 1;  // Invalid PID should fail with ECHILD
        }
    }
    
    return 1;  // All tests passed
} 