// Simple toy library whose functions will be interposed by the grate.
#include <stdio.h>
#include <stdlib.h>


int toy_add(int a, int b) {
    printf("[libtoy] toy_add(%d, %d) — this should NOT print if interposed\n", a, b);
    return a + b;
}

int toy_mul(int a, int b) {
    printf("[libtoy] toy_mul(%d, %d) — this should NOT print if interposed\n", a, b);
    return a * b;
}

// --- Functions added for Stage-3 marshalling tests ---

// toy_buf_checksum: sums byte values of b->data[0..b->len].
// Used by the nested-struct grate test.
struct toy_buffer {
    char    *data;  // offset 0 (wasm32: uint32_t ptr)
    unsigned len;   // offset 4
};

int toy_buf_checksum(const struct toy_buffer *b) {
    int sum = 0;
    for (unsigned i = 0; i < b->len; i++)
        sum += (unsigned char)b->data[i];
    return sum;
}

// toy_ctx_*: opaque context for handle-table tests.
// The grate intercepts these and maintains its own objects; the source cage
// receives only an opaque token, never a real pointer.
struct _toy_ctx { int val; };

void *toy_ctx_create(int val) {
    struct _toy_ctx *ctx = malloc(sizeof(*ctx));
    ctx->val = val;
    return ctx;
}

int toy_ctx_get_val(void *ctx) {
    return ((struct _toy_ctx *)ctx)->val;
}

void toy_ctx_close(void *ctx) {
    free(ctx);
}
