#include <stdio.h>

int main(int argc, char *argv[]) {
    // Command line inputs arent currently supported by the test harness.
    /*
    if (argc < 2) {
        printf("Usage: %s <your_argument>\n", argv[0]);
        return 1;
    }
    */

    printf("Received argument: %s\n", argv[0]);
    return 0;
}
