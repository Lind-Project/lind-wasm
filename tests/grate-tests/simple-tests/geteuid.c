#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret = geteuid();
    if (ret != 10) {
        fprintf(stderr, "[Cage | geteuid] FAIL: expected 10, got %d\n", ret);
        exit(EXIT_FAILURE);
    }
    printf("[Cage | geteuid] PASS: geteuid ret = %d\n", ret);
    return 0;
}
