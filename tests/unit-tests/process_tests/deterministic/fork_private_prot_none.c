#include <assert.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int *private = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(private != MAP_FAILED);
    *private = 42;
    assert(mprotect(private, 4096, PROT_NONE) == 0);

    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        if (mprotect(private, 4096, PROT_READ | PROT_WRITE) != 0 || *private != 42) {
            _exit(1);
        }
        *private = 77;
        _exit(0);
    }

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    assert(mprotect(private, 4096, PROT_READ | PROT_WRITE) == 0);
    assert(*private == 42);
    assert(munmap(private, 4096) == 0);
    return 0;
}
