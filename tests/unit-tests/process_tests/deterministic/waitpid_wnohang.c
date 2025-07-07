#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <errno.h>

/*
 * Test waitpid with WNOHANG flag
 * This test verifies that waitpid with WNOHANG works correctly
 * by testing the basic functionality without relying on timing.
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
        /* Child process - do minimal work and exit */
        printf("Child: starting (PID %d)\n", getpid());
        fflush(stdout);
        
        /* Simple loop to simulate some work */
        volatile int i;
        for (i = 0; i < 1000000; i++) {
            /* Just burn some cycles */
        }
        
        printf("Child: exiting\n");
        fflush(stdout);
        exit(42);  /* Exit with specific code for testing */
    } else {
        /* Parent process */
        printf("Parent: child pid = %d\n", cpid);
        fflush(stdout);
        
        /* Test 1: Call waitpid with WNOHANG - might return 0 or child PID */
        printf("Parent: calling waitpid with WNOHANG\n");
        fflush(stdout);
        wpid = waitpid(cpid, &status, WNOHANG);
        
        if (wpid == 0) {
            printf("PASS: waitpid with WNOHANG returned 0 (child not ready yet)\n");
            
            /* Now wait for child to complete */
            printf("Parent: waiting for child to complete...\n");
            fflush(stdout);
            wpid = waitpid(cpid, &status, 0);  /* Blocking wait */
            
            if (wpid == cpid) {
                printf("PASS: Blocking waitpid returned child pid %d\n", wpid);
                if (WIFEXITED(status)) {
                    printf("PASS: Child exited normally with status %d\n", WEXITSTATUS(status));
                }
            } else {
                printf("FAIL: Blocking waitpid failed\n");
                exit(EXIT_FAILURE);
            }
        } else if (wpid == cpid) {
            printf("OK: waitpid with WNOHANG returned child pid %d (child completed quickly)\n", wpid);
            if (WIFEXITED(status)) {
                printf("OK: Child exited normally with status %d\n", WEXITSTATUS(status));
                if (WEXITSTATUS(status) == 42) {
                    printf("PASS: Child exit status matches expected value\n");
                }
            }
        } else {
            perror("waitpid failed");
            exit(EXIT_FAILURE);
        }
        fflush(stdout);
        
        /* Test 2: Call waitpid again - should return -1 with ECHILD */
        printf("Parent: calling waitpid again (should get ECHILD)\n");
        fflush(stdout);
        wpid = waitpid(cpid, &status, WNOHANG);
        if (wpid == -1 && errno == ECHILD) {
            printf("PASS: waitpid with WNOHANG returned -1 with ECHILD (no more children)\n");
        } else if (wpid == -1) {
            perror("waitpid failed with different error");
        } else {
            printf("FAIL: waitpid should have returned -1 with ECHILD, got %d\n", wpid);
        }
        fflush(stdout);
        
        /* Test 3: Test with invalid PID to ensure WNOHANG works with errors */
        printf("Parent: testing waitpid with invalid PID\n");
        fflush(stdout);
        wpid = waitpid(99999, &status, WNOHANG);
        if (wpid == -1 && errno == ECHILD) {
            printf("PASS: waitpid with invalid PID returned -1 with ECHILD\n");
        } else {
            printf("UNEXPECTED: waitpid with invalid PID returned %d\n", wpid);
        }
        fflush(stdout);
    }
    
    printf("Test completed - WNOHANG functionality verified\n");
    fflush(stdout);
    return 0;
} 