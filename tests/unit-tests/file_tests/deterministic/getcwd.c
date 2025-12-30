#include <unistd.h>
#include <stdio.h>
#include <assert.h>
#include <string.h>

int main() {
  int buffersize = 256;
  char cwd[buffersize];

  char* result = getcwd(cwd, sizeof(cwd));

  if (result == NULL)
    perror("getcwd() error");
  else
  {
    assert(result == cwd);
    assert(strcmp(cwd, "/") == 0);
  }
  
  printf("getcwd test: PASS\n");

  return 0;
}
