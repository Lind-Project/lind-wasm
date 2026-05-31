// Cage for auto-memcpy marshalling test.
// Calls memcpy from libc and verifies that the intercepted version correctly
// copies data (proving copy-in, copy-out, and return-alias all work).
#include <stdio.h>
#include <string.h>

extern void *memcpy(void *dest, const void *src, size_t n);

int main(void) {
    const char src[] = "hello, lind!";
    char dst[32] = {0};

    void *ret = memcpy(dst, src, sizeof(src));

    // Verify copy-out: dst should contain the source string
    if (memcmp(dst, src, sizeof(src)) != 0) {
        fprintf(stderr, "[Cage|auto-memcpy] FAIL: dst mismatch after memcpy\n");
        return 1;
    }
    // Verify return alias: returned pointer should equal dst
    if (ret != (void *)dst) {
        fprintf(stderr, "[Cage|auto-memcpy] FAIL: return value %p != dst %p\n",
                ret, (void *)dst);
        return 1;
    }
    printf("[Cage|auto-memcpy] PASS: memcpy copied \"%s\", return == dst\n", dst);
    return 0;
}
