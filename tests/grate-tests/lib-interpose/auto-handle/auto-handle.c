// Cage for auto-handle marshalling test.
// Calls toy_ctx_create(42), toy_ctx_get_val, toy_ctx_close from libtoy.
// The grate intercepts all three: create returns an opaque token, get_val
// translates the token to the real object and returns 42, close releases it.
#include <stdio.h>

extern void *toy_ctx_create(int val);
extern int   toy_ctx_get_val(void *ctx);
extern void  toy_ctx_close(void *ctx);

int main(void) {
    void *ctx = toy_ctx_create(42);
    if (ctx == NULL) {
        fprintf(stderr, "[Cage|auto-handle] FAIL: toy_ctx_create returned NULL\n");
        return 1;
    }

    int val = toy_ctx_get_val(ctx);
    if (val != 42) {
        fprintf(stderr, "[Cage|auto-handle] FAIL: toy_ctx_get_val = %d, expected 42\n", val);
        return 1;
    }

    toy_ctx_close(ctx);

    printf("[Cage|auto-handle] PASS: create/get_val/close round-trip, val=%d\n", val);
    return 0;
}
