#include <assert.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>

int main() {
    // Create a MAP_SHARED anonymous mmap region
    void *p = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                   MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    assert(p != MAP_FAILED);

    // Store an int flag at p, initialize to 0
    int *flag = (int *)p;
    *flag = 0;

    // Fork
    pid_t pid = fork();
    assert(pid >= 0);

    if (pid == 0) {
        // Child: write a deterministic value
        *flag = 12345;
        _exit(0);
    } else {
        // Parent: wait for child
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);

        // Assert that the shared memory was updated by child
        assert(*flag == 12345);

        // Munmap and assert success
        assert(munmap(p, 4096) == 0);
    }

    return 0;
}
