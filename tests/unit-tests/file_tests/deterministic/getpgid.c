#include <stdio.h>
#include <unistd.h>
#include <assert.h>

int main(void) {
    pid_t pgid = getpgid(0);
    assert(pgid > 0);

    printf("getpgid: all tests passed\n");
    return 0;
}
