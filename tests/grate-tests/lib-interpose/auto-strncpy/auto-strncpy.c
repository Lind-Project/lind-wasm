// Cage for auto-strncpy marshalling test.
// Calls strncpy from libc and verifies copy and return alias.
#include <stdio.h>
#include <string.h>

extern char *strncpy(char *dest, const char *src, size_t n);

int main(void) {
    const char src[] = "lind-wasm";
    char dst[32] = {0};

    char *ret = strncpy(dst, src, sizeof(dst));

    if (strncmp(dst, src, strlen(src)) != 0) {
        fprintf(stderr, "[Cage|auto-strncpy] FAIL: dst mismatch: got \"%s\"\n", dst);
        return 1;
    }
    if (ret != dst) {
        fprintf(stderr, "[Cage|auto-strncpy] FAIL: return %p != dst %p\n",
                (void *)ret, (void *)dst);
        return 1;
    }
    printf("[Cage|auto-strncpy] PASS: strncpy produced \"%s\", return == dst\n", dst);
    return 0;
}
