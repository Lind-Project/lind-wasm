#include <stdio.h>

void hello(const char *name) {
    printf("Hello, %s! (from shared library)\n", name);
}
