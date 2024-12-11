#include <stdio.h>

int main(int argc, char *argv[], char *envp[]) {
    // Print command-line arguments
    printf("Command-line arguments:\n");
    for (int i = 0; i < argc; i++) {
        printf("argv[%d]: %s\n", i, argv[i]);
    }

    // Print environment variables
    printf("\nEnvironment variables:\n");
    for (char **env = envp; *env != NULL; env++) {
        printf("%s\n", *env);
    }

    return 0;
}

