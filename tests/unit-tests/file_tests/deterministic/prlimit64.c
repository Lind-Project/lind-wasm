#include <stdio.h>
#include <sys/resource.h>

int main(void) {
    struct rlimit rl;

    int ret = getrlimit(RLIMIT_NOFILE, &rl);
    if (ret != 0 || rl.rlim_cur == 0 || rl.rlim_max == 0) {
        printf("FAIL\n");
        return 1;
    }

    ret = getrlimit(RLIMIT_STACK, &rl);
    if (ret != 0 || rl.rlim_cur == 0) {
        printf("FAIL\n");
        return 1;
    }

    ret = getrlimit(RLIMIT_AS, &rl);
    if (ret != 0 || rl.rlim_cur == 0) {
        printf("FAIL\n");
        return 1;
    }

    ret = getrlimit(RLIMIT_DATA, &rl);
    if (ret != 0 || rl.rlim_cur == 0) {
        printf("FAIL\n");
        return 1;
    }

    printf("All prlimit64 tests passed\n");
    return 0;
}
