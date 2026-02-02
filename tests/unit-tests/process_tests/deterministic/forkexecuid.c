#include <sys/types.h>
#include <unistd.h>
#include <stdio.h>

#include <sys/types.h>
#include <sys/wait.h>
#include <assert.h>

// NOTE: This test assumes the test environment runs as root (UID/GID = 0).                                                                                                
// If tests run as non-root, these assertions will fail even if fork/exec work correctly.                                                                                  
#define ROOT_UID 0                                                                                                                                                         
#define ROOT_GID 0 

int main(void) {
  assert(getgid() == ROOT_GID && "parent gid should be 0");                                                                                                                       
  assert(getuid() == ROOT_UID && "parent uid should be 0");                                                                                                                       
  assert(getegid() == ROOT_GID && "parent egid should be 0");                                                                                                                     
  assert(geteuid() == ROOT_UID && "parent euid should be 0"); 
  
  pid_t pid = fork();                                                                                                                                                      
  assert(pid != -1 && "fork should succeed");

  if (pid == 0) {                                                                                                                                                          
    // Child asserts IDs preserved after fork                                                                                                                              
    assert(getgid() == ROOT_GID && "child gid should be 0 after fork");                                                                                                           
    assert(getuid() == ROOT_UID && "child uid should be 0 after fork");                                                                                                           
    assert(getegid() == ROOT_GID && "child egid should be 0 after fork");                                                                                                         
    assert(geteuid() == ROOT_UID && "child euid should be 0 after fork");                                                                                                         
                                                                                                                                                                                                                                                                                                        
    char* arr[] = {"getuid", NULL};                                                                                                                                        
    execv("automated_tests/getuid", arr);                                                                                                                                  
                                                                                                                                                                                                                                                                                                                          
    assert(0 && "execv should not return");                                                                                                                                
  }
  int status;                                                                                                                                                              
  wait(&status);                                                                                                                                                           
  assert(WIFEXITED(status) && "child should exit normally");                                                                                                               
  assert(WEXITSTATUS(status) == 0 && "child should exit with status 0");

  return 0;
}
