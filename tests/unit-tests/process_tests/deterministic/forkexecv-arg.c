/*
* Before running this test:
*   1. Make sure to compile the target program (hello) using your desired toolchain.
*   2. Copy the compiled binary to $LIND_FS_ROOT.
*   3. IMPORTANT: Rename the binary to "hello-arg" (no .wasm or other extensions).
*
* The executable must be accessible at: $LIND_FS_ROOT/hello-arg
*/

#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>

int main() {
    pid_t pid = fork();

    if (pid == 0) {
        // child process: call execv with argument
        char *args[] = {"./hello-arg", "hello_from_parent", NULL};
        execv("./hello-arg", args);
        perror("execv failed");  // only runs if execv fails
    } else {
        // parent process
        wait(NULL);  // wait for child to finish
    }

    return 0;
}
