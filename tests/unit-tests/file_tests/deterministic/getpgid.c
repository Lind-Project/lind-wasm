#include <stdio.h>
#include <unistd.h>
#include <assert.h>

int main(void) {
    pid_t pid = getpid();
    assert(pid > 0);

    /* getpgid(0) returns own process group, which in lind is the cageid */
    pid_t pgid = getpgid(0);
    assert(pgid > 0);
    assert(pgid == pid);

    /* getpgid(getpid()) should return the same */
    pid_t pgid2 = getpgid(pid);
    assert(pgid2 == pid);

    printf("getpgid: all tests passed\n");
    return 0;
}
