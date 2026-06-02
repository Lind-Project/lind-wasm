// Cage for auto-compress2 marshalling test.
// Calls compress2() from libz; grate intercepts, writes "LIND" into dest,
// sets *destLen = 4. Cage verifies dest == "LIND" and *destLen == 4.
#include <stdio.h>
#include <string.h>
#include <stdint.h>

// Minimal zlib types for wasm32
typedef unsigned char  Bytef;
typedef unsigned long  uLong;
typedef unsigned long  uLongf;

extern int compress2(Bytef *dest, uLongf *destLen,
                     const Bytef *source, uLong sourceLen, int level);

int main(void) {
    const char src[] = "hello, lind compress test";
    char       dst[256];
    uLongf     destLen = sizeof(dst);

    int ret = compress2((Bytef*)dst, &destLen,
                        (const Bytef*)src, sizeof(src), 6);

    if (ret != 0) {
        fprintf(stderr, "[Cage|auto-compress2] FAIL: compress2 returned %d\n", ret);
        return 1;
    }
    if (destLen != 4) {
        fprintf(stderr, "[Cage|auto-compress2] FAIL: destLen=%lu, expected 4\n", destLen);
        return 1;
    }
    if (memcmp(dst, "LIND", 4) != 0) {
        fprintf(stderr, "[Cage|auto-compress2] FAIL: dst != \"LIND\"\n");
        return 1;
    }
    printf("[Cage|auto-compress2] PASS: got \"%.*s\" destLen=%lu\n",
           (int)destLen, dst, destLen);
    return 0;
}
