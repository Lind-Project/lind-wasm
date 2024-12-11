#include <stdio.h>

int main(int argc, char *argv[]) {
    // Print command-line arguments
    printf("Command-line arguments:\n");
    for (int i = 0; i < argc; i++) {
        printf("argv[%d]: %s\n", i, argv[i]);
    }

    return 0;
}

