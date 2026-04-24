#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <assert.h>

int main(void) {
    /* Test 1: basic sbrk(0) returns current break */
    void *initial = sbrk(0);
    assert(initial != (void *)-1);

    /* Test 2: small increment */
    void *p = sbrk(4096);
    assert(p != (void *)-1);
    void *after = sbrk(0);
    assert(after == p + 4096);

    /* write to the allocated region */
    memset(p, 0xAA, 4096);

    /* Test 3: multiple small increments */
    for (int i = 0; i < 100; i++) {
        void *before = sbrk(0);
        void *r = sbrk(4096);
        assert(r != (void *)-1);
        assert(r == before);
        /* touch the memory */
        memset(r, (char)i, 4096);
    }

    /* Test 4: large increment */
    void *big = sbrk(1024 * 1024);
    assert(big != (void *)-1);
    memset(big, 0xBB, 1024 * 1024);

    /* Test 5: malloc stress (uses brk internally) */
    void *ptrs[500];
    for (int i = 0; i < 500; i++) {
        ptrs[i] = malloc(4096);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], (char)i, 4096);
    }

    /* free in reverse */
    for (int i = 499; i >= 0; i--) {
        free(ptrs[i]);
    }

    /* Test 6: interleaved malloc/free to fragment heap */
    for (int i = 0; i < 500; i++) {
        ptrs[i] = malloc(4096);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], (char)i, 4096);
    }
    /* free every other */
    for (int i = 0; i < 500; i += 2) {
        free(ptrs[i]);
        ptrs[i] = NULL;
    }
    /* reallocate into the gaps */
    for (int i = 0; i < 500; i += 2) {
        ptrs[i] = malloc(4096);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], 0xCC, 4096);
    }
    /* free all */
    for (int i = 0; i < 500; i++) {
        free(ptrs[i]);
    }

    /* Test 7: growing allocations */
    for (int i = 1; i <= 20; i++) {
        size_t sz = i * 64 * 1024;
        void *g = malloc(sz);
        assert(g != NULL);
        memset(g, 0xDD, sz);
        free(g);
    }

    /* Test 8: many tiny allocations */
    for (int i = 0; i < 500; i++) {
        ptrs[i] = malloc(16);
        assert(ptrs[i] != NULL);
    }
    for (int i = 0; i < 500; i++) {
        free(ptrs[i]);
    }

    /* Test 9: verify break is still sane */
    void *final_brk = sbrk(0);
    assert(final_brk != (void *)-1);
    assert(final_brk >= initial);

    printf("brk_stress: all tests passed\n");
    return 0;
}
