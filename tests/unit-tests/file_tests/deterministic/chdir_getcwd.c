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
  }

  int cwdlen = strlen(cwd);
  //---------------------------
  /* Chdir into /test causes issues on native as we don't have the permissions to create a folder in /. 
  Creating a folder under $cwd would be better. We are currently using the test suite's folder. -Kapkic */
  chdir("automated_tests/"); 
  //---------------------------
  char newcwd[buffersize];
  result = getcwd(newcwd, sizeof(newcwd));

  if (result == NULL)
    perror("getcwd() error");
  else
  {
    assert(result == newcwd);

    assert(strncmp(cwd, newcwd, cwdlen) == 0);
    if(cwd[cwdlen - 1] == '/')
      assert(strcmp(newcwd + strlen(cwd), "automated_tests") == 0);
    else
      assert(strcmp(newcwd + strlen(cwd), "/automated_tests") == 0);
  }

  printf("chdir test: PASS\n");

  return 0;
}
