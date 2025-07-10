#include <stdio.h>

int main(int argc, char *argv[]) {
    if (argc < 2) {
        printf("Usage: %s <your_argument>\n", argv[0]);
        return 1;
    }

    printf("Received argument: %s\n", argv[1]);
    return 0;
}
