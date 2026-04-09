#include <stdio.h>
#include <dlfcn.h>

int main() {
    const char *invalid_path = "/nonexistent/library.so";

    // Attempt to open the shared library
    void *handle = dlopen(invalid_path, RTLD_LAZY);

    if (!handle) {
        // Print the error message
        const char *error = dlerror();
        if (error) {
            printf("dlopen failed: %s\n", error);
        } else {
            printf("dlopen failed: unknown error\n");
        }
        return 1;
    }

    // This should not happen with an invalid path
    printf("Library loaded successfully (unexpected)\n");

    dlclose(handle);
    return 0;
}