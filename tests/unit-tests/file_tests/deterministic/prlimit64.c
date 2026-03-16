#include <stdio.h>
#include <sys/resource.h>
#include <stdlib.h>

int main(void) {
    struct rlimit rl;

    int ret = getrlimit(RLIMIT_NOFILE, &rl);
    if (ret != 0) {
        printf("FAIL: getrlimit returned %d\n", ret);
        return 1;
    }
    if (rl.rlim_cur == 0) {
        printf("FAIL: rlim_cur is 0\n");
        return 1;
    }
    if (rl.rlim_max == 0) {
        printf("FAIL: rlim_max is 0\n");
        return 1;
    }
    printf("RLIMIT_NOFILE: cur=%lu max=%lu\n", (unsigned long)rl.rlim_cur, (unsigned long)rl.rlim_max);

    ret = getrlimit(RLIMIT_STACK, &rl);
    if (ret != 0) {
        printf("FAIL: getrlimit RLIMIT_STACK returned %d\n", ret);
        return 1;
    }
    if (rl.rlim_cur == 0) {
        printf("FAIL: RLIMIT_STACK rlim_cur is 0\n");
        return 1;
    }
    printf("RLIMIT_STACK: cur=%lu max=%lu\n", (unsigned long)rl.rlim_cur, (unsigned long)rl.rlim_max);

    printf("All prlimit64 tests passed\n");
    return 0;
}
