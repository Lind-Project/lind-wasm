#include <stdio.h>
#include <stdlib.h>
#include <dlfcn.h>

int main(void) {
    void *handle;
    void (*hello_func)(const char*);
    char *error;

    /* Load the math library */
    handle = dlopen("lib.wasm", RTLD_LAZY);
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

    hello_func("main module");

    /* Close the library */
    dlclose(handle);
    return 0;
}
