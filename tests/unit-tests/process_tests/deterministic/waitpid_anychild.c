#include <stdio.h>                                                                                                                                                                           
#include <stdlib.h>                                                                                                                                                                          
#include <sys/wait.h>                                                                                                                                                                        
#include <unistd.h>                                                                                                                                                                          
#include <errno.h>

/*
Test waitpid() with pid=-1 (wait for any child)
This is a regression test for bug: https://github.com/Lind-Project/lind-wasm/issues/543
*/
int main()
{
    pid_t child_pid, result;
    int status;

    /* Create a child process */
    child_pid = fork();

    if (child_pid == -1) {
        printf("fork failed\n");
        return 1;
    }

    if (child_pid == 0) {                                                                                                                                                                    
          /* Child process - exit with a known status */                                                                                                                                       
          printf("Child PID=%d, exiting with status 42\n", getpid());                                                                                                                         
          exit(42);                                                                                                                                                                            
    }  

    result = waitpid(-1, &status, 0);

    if (result == -1) {
        printf("waitpid return -1, errono=%d\n", errno);
        return 1;
    }

    /* Verify we got our child's PID */                                                                                                                                                      
    if (result != child_pid) {                                                                                                                                                               
        printf("FAIL: Expected PID %d, got %d\n", child_pid, result);                                                                                                                        
        return 1;                                                                                                                                                                            
    }

    /* Check exit status (handle both POSIX and raw formats) */                                                                                                                              
    int exit_code;                                                                                                                                                                           
    if (WIFEXITED(status)) {                                                                                                                                                                 
        /* POSIX format */                                                                                                                                                                   
        exit_code = WEXITSTATUS(status);                                                                                                                                                     
    } else {                                                                                                                                                                                 
        /* WebAssembly raw format */                                                                                                                                                         
        exit_code = status;                                                                                                                                                                  
    }

    if (exit_code != 42) {                                                                                                                                                                   
        printf("Test Failed: Expected exit code 42, got %d\n", exit_code);                                                                                                                          
        return 1;                                                                                                                                                                            
    }                                                                                                                                                                                        
                                                                                                                                                                                               
    printf("Test Passed: waitpid(-1) correctly waited for child\n");                                                                                                                                
    return 0; 
}

