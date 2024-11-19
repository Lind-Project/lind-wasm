#include <stdio.h>

void __attribute__((destructor)) cleanup_init(void) {
    // Conceptually clean up after initialization.
    printf("Cleanup after initialization.\n");
}

void __attribute__((destructor)) cleanup_fini(void) {
    // Conceptually clean up after finalization.
    printf("Cleanup after finalization.\n");
}
