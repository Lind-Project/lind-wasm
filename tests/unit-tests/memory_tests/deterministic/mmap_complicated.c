#include <assert.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

int main() {
    size_t mem_size = sizeof(int);
    
    // Create two anonymous mmaps before fork
    // 1) shared region
    int *shared_int = (int *)mmap(NULL, mem_size, PROT_READ | PROT_WRITE, 
                                   MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    assert(shared_int != MAP_FAILED);
    
    // 2) private region
    int *private_int = (int *)mmap(NULL, mem_size, PROT_READ | PROT_WRITE, 
                                    MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(private_int != MAP_FAILED);
    
    // Initialize
    *shared_int = 1;
    *private_int = 1;
    
    // Fork
    pid_t pid = fork();
    assert(pid >= 0);
    
    if (pid == 0) {
        // Child: set shared_int = 42, private_int = 99
        *shared_int = 42;
        *private_int = 99;
        _exit(0);
    } else {
        // Parent: wait for child
        int status;
        pid_t waited_pid = waitpid(pid, &status, 0);
        assert(waited_pid >= 0);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
        
        // Assert shared_int == 42 (should reflect child write)
        assert(*shared_int == 42);
        
        // Assert private_int == 1 (should NOT reflect child write)
        assert(*private_int == 1);
        
        // Munmap both and assert success
        assert(munmap(shared_int, mem_size) == 0);
        assert(munmap(private_int, mem_size) == 0);
    }
    
    return 0;
}
