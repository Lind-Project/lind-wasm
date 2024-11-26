#include <stdlib.h>
#include <unistd.h>

int main() {

    // small chunks should use sbrk only
    char *buf = malloc(0x10000);

    if (buf == NULL) {
        return -1;
    }

    buf = malloc(0x100);

    // Try write/read on the allocated memory
    *((int *)buf) = 10;
    int myInt = *buf;

    // Larger chunks should trigger the mmap path of malloc
    buf = malloc(0x100000);

    // Try accessing it
    *((int *)buf) = 12;
    myInt = *buf;

    return 0;
}