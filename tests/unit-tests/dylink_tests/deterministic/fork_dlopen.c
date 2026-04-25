#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <dlfcn.h>

int main(void) {
    void *handle;
    void (*hello_func)(const char*);
    char *error;

    int pid = fork();

    if(pid == 0)
    {
        // child routine

        /* Load the math library */
        handle = dlopen("lib.cwasm", RTLD_LAZY);
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

        /* Load the math library */
        handle = dlopen("lib.cwasm", RTLD_LAZY);
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
