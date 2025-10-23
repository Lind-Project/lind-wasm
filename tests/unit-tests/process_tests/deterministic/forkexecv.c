/*
* Before running this test:
*   1. Make sure to compile the target program (hello) using your desired toolchain.
*   2. Copy the compiled binary to $LIND_FS_ROOT.
*   3. IMPORTANT: Rename the binary to "hello" (no .wasm or other extensions).
*
* The executable must be accessible at: $LIND_FS_ROOT/hello
*/

#include <sys/types.h>
#include <unistd.h>
#include <stdio.h>
#include <sys/wait.h>

int main(void)
{
  pid_t pid;

  if ((pid = fork()) == -1) {
    perror("fork error");
  }
  else if (pid == 0) {
    char* arr[] = {"hello", NULL};
    execv("automated_tests/hello", arr);
  }
  wait(NULL);
}