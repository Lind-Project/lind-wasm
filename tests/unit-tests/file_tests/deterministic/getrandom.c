#define _GNU_SOURCE
#include <stdio.h>
#include <unistd.h>
#include <sys/random.h>
#include <errno.h>
#include <string.h>
#include <stdbool.h>

static int fill_with_random(unsigned char *buf, size_t len) {
    size_t filled = 0;

    while (filled < len) {
        ssize_t ret = getrandom(buf + filled, len - filled, 0);
        if (ret < 0) {
            if (errno == EINTR) {
                // Interrupted by signal, retry
                continue;
            }
            perror("getrandom");
            return -1;
        }
        if (ret == 0) {
            // Should not happen, treat as error
            fprintf(stderr, "getrandom returned 0 bytes unexpectedly\n");
            return -1;
        }
        filled += (size_t)ret;
    }

    return 0;
}

int main(void) {
    unsigned char buf1[32];
    unsigned char buf2[32];

    // 1. Fill first buffer
    if (fill_with_random(buf1, sizeof(buf1)) < 0) {
        fprintf(stderr, "getrandom basic test: FAIL (error filling buf1)\n");
        return 1;
    }

    // 2. Fill second buffer
    if (fill_with_random(buf2, sizeof(buf2)) < 0) {
        fprintf(stderr, "getrandom basic test: FAIL (error filling buf2)\n");
        return 1;
    }

    // 3. Check that buf1 is not all zeros (very basic sanity check)
    bool all_zero = true;
    for (size_t i = 0; i < sizeof(buf1); i++) {
        if (buf1[i] != 0) {
            all_zero = false;
            break;
        }
    }
    if (all_zero) {
        fprintf(stderr, "getrandom basic test: FAIL (buf1 is all zeros)\n");
        return 1;
    }

    // 4. Check that buf1 and buf2 differ (they *should* be different)
    if (memcmp(buf1, buf2, sizeof(buf1)) == 0) {
        fprintf(stderr, "getrandom basic test: FAIL (two buffers are identical)\n");
        return 1;
    }

    // If we reached here, everything looks fine.
    printf("getrandom basic test: PASS\n");
    return 0;
}
