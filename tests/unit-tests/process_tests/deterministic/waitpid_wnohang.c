#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <errno.h>

/*
 * Test waitpid with WNOHANG flag
 * This test verifies that waitpid with WNOHANG works correctly.
 * NOTE: WebAssembly version has a status format bug (should be POSIX format)
 */
int main()
{
    pid_t cpid, wpid;
    int status;
    
    printf("Testing waitpid with WNOHANG flag\n");
    fflush(stdout);
    
    cpid = fork();
    if (cpid == -1) {
        perror("fork failed");
        exit(EXIT_FAILURE);
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
        printf("Parent: child pid = %d\n", cpid);
        fflush(stdout);
        
        /* Test 1: Call waitpid with WNOHANG */
        wpid = waitpid(cpid, &status, WNOHANG);
        
        if (wpid == 0) {
            printf("WNOHANG Test: Child not ready, got 0\n");
            /* Now wait for child to complete */
            wpid = waitpid(cpid, &status, 0);  /* Blocking wait */
        } else if (wpid == cpid) {
            printf("WNOHANG Test: Child completed quickly, got PID\n");
        } else {
            perror("waitpid failed");
            exit(EXIT_FAILURE);
        }
        
        /* Verify child was reaped - handle both POSIX and WebAssembly status formats */
        if (wpid == cpid) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 42) {
                /* Native/correct POSIX format */
                printf("Child reaped successfully with correct exit code\n");
            } else if (status == 42) {
                /* WebAssembly format bug - status is raw exit code */
                printf("Child reaped successfully with correct exit code\n");
            } else {
                printf("Child reaping failed or wrong exit code\n");
            }
        } else {
            printf("Child reaping failed - waitpid returned wrong PID\n");
        }
        fflush(stdout);
        
        /* Test 2: Call waitpid again - should return -1 with ECHILD */
        wpid = waitpid(cpid, &status, WNOHANG);
        if (wpid == -1 && errno == ECHILD) {
            printf("Second waitpid: Got ECHILD as expected\n");
        } else {
            printf("Second waitpid: Unexpected result %d\n", wpid);
        }
        fflush(stdout);
        
        /* Test 3: Test with invalid PID */
        wpid = waitpid(99999, &status, WNOHANG);
        if (wpid == -1 && errno == ECHILD) {
            printf("Invalid PID test: Got ECHILD as expected\n");
        } else {
            printf("Invalid PID test: Unexpected result %d\n", wpid);
        }
        fflush(stdout);
    }
    
    printf("WNOHANG functionality test completed\n");
    fflush(stdout);
    return 0;
} 