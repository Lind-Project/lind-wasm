#include <stdlib.h>
#include <unistd.h>
#include <stdio.h>

int main() {
    // try with extremely large malloc
    char *buf = malloc(0x10000000);

    *buf = 42;
    printf("%p: %d\n", buf, *buf);

    return 0;
}