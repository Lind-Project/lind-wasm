#include <stdio.h>
#include <unistd.h>
#include <dlfcn.h>

int main(void) {
    printf("=== STEP 2: Simple execve Test ===\n");
    printf("Before execve call\n");
    fflush(stdout);
    
    char* arr[] = {"hello", NULL};
    
    printf("arr address: %p\n", (void*)arr);
    printf("arr[0] address: %p\n", (void*)arr[0]);
    printf("arr[0] string: %s\n", arr[0]);
    
    // Check if execve symbol exists
    void *execve_ptr = dlsym(RTLD_DEFAULT, "execve");
    printf("execve function pointer: %p\n", execve_ptr);
    fflush(stdout);
    
    printf("About to call execve...\n");
    fflush(stdout);
    
    int result = execve("automated_tests/hello", arr, NULL);
    
    perror("execve failed");
    printf("execve returned: %d\n", result);
    return 1;
}