#include <assert.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int *shared = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                       MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    assert(shared != MAP_FAILED);
    *shared = 42;
    assert(mprotect(shared, 4096, PROT_NONE) == 0);

    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        if (mprotect(shared, 4096, PROT_READ | PROT_WRITE) != 0 || *shared != 42) {
            _exit(1);
        }
        *shared = 77;
        _exit(0);
    }

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    assert(mprotect(shared, 4096, PROT_READ | PROT_WRITE) == 0);
    assert(*shared == 77);
    assert(munmap(shared, 4096) == 0);
    return 0;
}
