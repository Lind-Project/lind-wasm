// demo.c — a tiny library that exercises each thing marshal-infer infers.
// Walk through the pipeline with ./run.sh; see README.md.
#include <stdlib.h>

// (1) all-scalar: nothing to marshal.
int add(int a, int b) {
    return a + b;
}

// (2) buf + len: dst is written (OUT), src is const (IN); both sized by n.
//     -> dst dir=out size=from_arg(2), src dir=in size=from_arg(2), n SCALAR.
void encode(unsigned char *dst, const unsigned char *src, unsigned long n) {
    for (unsigned long i = 0; i < n; i++)
        dst[i] = (unsigned char)(src[i] ^ 0x5A);
}

// (3) C string: const char* read to the NUL terminator.
//     -> s dir=in size=cstr.
unsigned long demo_strlen(const char *s) {
    unsigned long n = 0;
    while (s[n]) n++;
    return n;
}

// (4) nested struct passed by pointer: a (buf,len) pair *inside* a struct.
//     -> b PTR IN size=const(8); pointee STRUCT {data: PTR IN size=from_arg(field 1),
//        len: SCALAR}, both fields touched.
struct buffer {
    char    *data;  // wasm32 offset 0
    unsigned len;   // wasm32 offset 4
};
int checksum(const struct buffer *b) {
    int sum = 0;
    for (unsigned i = 0; i < b->len; i++)
        sum += (unsigned char)b->data[i];
    return sum;
}

// (5) opaque handle lifecycle: the API only ever exposes `void *`. The real
//     object must never be deep-copied across cages — it is a handle.
//     -> ctx_open ret=handle; ctx_read/ctx_close arg0 = HANDLE.
struct ctx { int state; };  // definition hidden from the public API

void *ctx_open(int seed) {
    struct ctx *c = malloc(sizeof(*c));
    c->state = seed;
    return c;
}
int ctx_read(void *c) {
    return ((struct ctx *)c)->state;
}
void ctx_close(void *c) {
    free(c);
}
