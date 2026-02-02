#include <sys/types.h>
#include <unistd.h>
#include <stdio.h>

#include <sys/types.h>
#include <sys/wait.h>
#include <assert.h>

int main(void) {
  assert(getgid() == 0 && "parent gid should be 0");                                                                                                                       
  assert(getuid() == 0 && "parent uid should be 0");                                                                                                                       
  assert(getegid() == 0 && "parent egid should be 0");                                                                                                                     
  assert(geteuid() == 0 && "parent euid should be 0"); 
  
  pid_t pid = fork();                                                                                                                                                      
  assert(pid != -1 && "fork should succeed");

  if (pid == 0) {                                                                                                                                                          
    // Child asserts IDs preserved after fork                                                                                                                              
    assert(getgid() == 0 && "child gid should be 0 after fork");                                                                                                           
    assert(getuid() == 0 && "child uid should be 0 after fork");                                                                                                           
    assert(getegid() == 0 && "child egid should be 0 after fork");                                                                                                         
    assert(geteuid() == 0 && "child euid should be 0 after fork");                                                                                                         
                                                                                                                                                                                                                                                                                                        
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
