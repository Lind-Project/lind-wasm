// main.c
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <dlfcn.h>

// This function will be called from the shared library
void hello_from_main(void) {
    printf("Hello from main executable!\n");
}

int main(void) {
    void *handle;
    void (*plugin_entry)(void);

    // Load the shared library
    handle = dlopen("./rdynamic_lib.cwasm", RTLD_NOW);
    if (!handle) {
        fprintf(stderr, "dlopen error: %s\n", dlerror());
        return 1;
    }

    // Resolve symbol from the plugin
    plugin_entry = dlsym(handle, "plugin_entry");
    if (!plugin_entry) {
        fprintf(stderr, "dlsym error: %s\n", dlerror());
        return 1;
    }

    int pid = fork();

    // Call into the plugin
    plugin_entry();

    dlclose(handle);
    return 0;
}
