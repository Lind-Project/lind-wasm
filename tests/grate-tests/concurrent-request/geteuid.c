#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret;
    for (int i = 0; i < 1000000; i++) {
        ret = geteuid();
        if (ret != 10) {
            fprintf(stderr, "[Cage | geteuid] FAIL: iteration %d, expected 10, got %d\n", i, ret);
            exit(EXIT_FAILURE);
        }
    }
    printf("[Cage | geteuid] PASS: 1000000 calls returned %d\n", ret);
    return 0;
}
