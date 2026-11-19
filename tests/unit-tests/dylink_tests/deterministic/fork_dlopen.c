#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <dlfcn.h>
#include <sys/wait.h>

int main(void) {
    void *handle;
    void (*hello_func)(const char*);
    char *error;

    int pid = fork();
    if (pid < 0) {
        perror("fork failed");
        return 1;
    }

    if(pid == 0)
    {
        // child routine

        /* Load the math library */
        handle = dlopen("lib.so", RTLD_LAZY);
        if (!handle) {
            fprintf(stderr, "dlopen failed: %s\n", dlerror());
            return 1;
        }

        /* Clear any existing error */
        dlerror();

        /* Get symbol */
        *(void **)(&hello_func) = dlsym(handle, "hello");
        error = dlerror();
        if (error) {
            fprintf(stderr, "dlsym failed: %s\n", error);
            dlclose(handle);
            return 1;
        }

        hello_func("child in main module");

        /* Close the library */
        dlclose(handle);
    }
    else
    {
        // parent routine

        // wait for the child to finish first so the output order is
        // deterministic (child line always comes before parent line)
        int status;
        if (waitpid(pid, &status, 0) < 0) {
            perror("waitpid failed");
            return 1;
        }

        /* Load the math library */
        handle = dlopen("lib.so", RTLD_LAZY);
        if (!handle) {
            fprintf(stderr, "dlopen failed: %s\n", dlerror());
            return 1;
        }

        /* Clear any existing error */
        dlerror();

        /* Get symbol */
        *(void **)(&hello_func) = dlsym(handle, "hello");
        error = dlerror();
        if (error) {
            fprintf(stderr, "dlsym failed: %s\n", error);
            dlclose(handle);
            return 1;
        }

        hello_func("parent in main module");

        /* Close the library */
        dlclose(handle);
    }

    return 0;
}
