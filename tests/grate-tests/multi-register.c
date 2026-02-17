#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret_euid = geteuid();
    int ret_uid = getuid();
    if (ret_euid != 10) {
        fprintf(stderr, "[Cage | multi-register] FAIL: geteuid expected 10, got %d\n", ret_euid);
        exit(EXIT_FAILURE);
    }
    if (ret_uid != 20) {
        fprintf(stderr, "[Cage | multi-register] FAIL: getuid expected 20, got %d\n", ret_uid);
        exit(EXIT_FAILURE);
    }
    printf("[Cage | multi-register] PASS: geteuid=%d, getuid=%d\n", ret_euid, ret_uid);
    return 0;
}
