// Cage for auto-memchr marshalling test.
// Calls memchr to find 'l' in "hello"; expects a pointer into the original
// source buffer (not a shadow pointer).
#include <stdio.h>
#include <string.h>
#include <stddef.h>

extern void *memchr(const void *s, int c, size_t n);

int main(void) {
    const char buf[] = "hello";
    void *p = memchr(buf, 'l', sizeof(buf));

    if (p == NULL) {
        fprintf(stderr, "[Cage|auto-memchr] FAIL: memchr returned NULL\n");
        return 1;
    }
    // p must point into buf, not into grate shadow memory
    ptrdiff_t off = (char *)p - buf;
    if (off != 2) {
        fprintf(stderr, "[Cage|auto-memchr] FAIL: offset=%td, expected 2\n", off);
        return 1;
    }
    printf("[Cage|auto-memchr] PASS: found 'l' at offset %td\n", off);
    return 0;
}
