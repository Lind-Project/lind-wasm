#include <stdio.h>
#include <unistd.h>
#include <stdlib.h>

int main(void) {
    const size_t PAGESIZE = 4096;

    /* Get current program break */
    void *orig = sbrk(0);
    if (orig == (void*)-1) {
        perror("sbrk(0)");
        return 2;
    }

    /* Request +1 page */
    void *want = (char*)orig + PAGESIZE;
    if (brk(want) != 0) {
        perror("brk(grow)");
        printf("FAIL: grow\n");
        return 1;
    }

    /* Verify break moved */
    void *now = sbrk(0);
    if (now != want) {
        printf("FAIL: break did not advance by one page\n");
        brk(orig);
        return 1;
    }

    /* Write/read test */
    unsigned char *base = (unsigned char*)orig;
    base[0] = 0xA5;
    ((unsigned char*)now)[-1] = 0x5A;

    if (base[0] != 0xA5 || ((unsigned char*)now)[-1] != 0x5A) {
        printf("FAIL: write/read test\n");
        brk(orig);
        return 1;
    }

    /* Shrink back */
    if (brk(orig) != 0) {
        perror("brk(shrink)");
        printf("FAIL: shrink\n");
        return 1;
    }

    printf("PASS\n");
    return 0;
}

