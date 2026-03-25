#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <assert.h>
#include <lind_syscall.h>

int main(int argc, char *argv[]) {
    printf("[Cage|interpose-register] In cage %d, about to register handler for geteuid\n", getpid());
    int ret_reg = register_handler(2, 107, 1, 0);
    if (ret_reg != 0) {
        fprintf(stderr, "[Cage|interpose-register] Failed to register handler for cage %d in "
                "grate %d with fn ptr addr: %llu, ret: %d\n",
                2, 1, 0ULL, ret_reg);
        assert(0);
    }
    int ret = geteuid();
    if (ret != 10) {
        fprintf(stderr, "[Cage|interpose-register] FAIL: expected 10, got %d\n", ret);
        assert(0);
    }
    printf("[Cage|interpose-register] PASS: geteuid ret = %d\n", ret);
    return 0;
}
