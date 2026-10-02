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

// toy_wide_sum7: 7 scalar args -- one raw ABI slot beyond the 6-slot
// interposition transport. Used to test that a portal for a wider-than-6
// signature is rejected at link time rather than silently truncated.
int toy_wide_sum7(int a, int b, int c, int d, int e, int f, int g) {
    printf("[libtoy] toy_wide_sum7 — this should NOT print if interposed\n");
    return a + b + c + d + e + f + g;
}

// toy_argv_len: sum of strlen of each element of a NULL-terminated argv array.
// Used by the ptr_array (argv) marshalling test.
int toy_argv_len(const char **argv) {
    int total = 0;
    for (int i = 0; argv[i] != 0; i++) {
        const char *s = argv[i];
        while (*s) { total++; s++; }
    }
    return total;
}

// --- Functions for the post-call pointer-provenance tests (LIND_RET_PTR_INTO_ARG,
// out_ptr_into_arg1, and struct-field OUT/cursor copy-back+fixup). ---

// toy_scan_buf: memchr-alike, returns a pointer into buf (or NULL).
// Used by the LIND_RET_PTR_INTO_ARG provenance test.
const char *toy_scan_buf(const char *buf, int c, unsigned n) {
    printf("[libtoy] toy_scan_buf — this should NOT print if interposed\n");
    for (unsigned i = 0; i < n; i++)
        if (buf[i] == (char)c) return &buf[i];
    return 0;
}

// toy_strtol_like: strtol-alike, writes an endptr into *nptr's own buffer.
// Used by the out_ptr_into_arg1 (OUT ptr-to-ptr) provenance test.
long toy_strtol_like(const char *nptr, char **endptr) {
    printf("[libtoy] toy_strtol_like — this should NOT print if interposed\n");
    long v = 0;
    const char *p = nptr;
    while (*p >= '0' && *p <= '9') { v = v * 10 + (*p - '0'); p++; }
    if (endptr) *endptr = (char *)p;
    return v;
}

// toy_stream_process: a minimal zlib-deflate-alike cursor-advance struct,
// used by the struct-field OUT/cursor copy-back+fixup provenance test
// (lind_marshal.h's _lind_post_struct Step 1/Step 3).
struct toy_stream { const char *next_in; unsigned avail_in; char *next_out; unsigned avail_out; };
int toy_stream_process(struct toy_stream *s) {
    printf("[libtoy] toy_stream_process — this should NOT print if interposed\n");
    unsigned n = s->avail_in < s->avail_out ? s->avail_in : s->avail_out;
    for (unsigned i = 0; i < n; i++) s->next_out[i] = s->next_in[i];
    s->next_out += n;
    s->avail_out -= n;
    return 0;
}

// toy_daxpy: cblas_daxpy-alike (Y += alpha*X, strided). Used to test
// LIND_SIZE_STRIDE_VECTOR (issue #26): naive n*elem_size sizing undersizes
// X/Y whenever the stride isn't 1, since real BLAS-style calls span
// elements 0, stride, 2*stride, ..., (n-1)*stride -- not just the first n.
// Returns int (unlike real cblas_daxpy's void) purely so a rejected call's
// LIND_GRATE_ERR sentinel has a return value to travel back in -- a void
// portal has no result slot for it to occupy at all.
int toy_daxpy(int n, double alpha, const double *x, int incx, double *y, int incy) {
    // Unlike this file's other toy_* functions, the stridevec_grate.c
    // handler for this one calls straight through to this real
    // implementation (on the shadow-marshalled buffers) -- so running here
    // is the expected, correct outcome, not a sign of a marshalling bug.
    for (int i = 0; i < n; i++)
        y[i * incy] += alpha * x[i * incx];
    return 0;
}

// toy_daxpy_ref: same computation as toy_daxpy, but n/incx/incy are passed
// by reference -- the Fortran BLAS calling convention, where every scalar
// argument is a pointer to its value rather than the value itself. Used to
// test LIND_SIZE_STRIDE_VECTOR's pointee-sourced extent operands.
int toy_daxpy_ref(const int *n, double alpha, const double *x, const int *incx,
                   double *y, const int *incy) {
    for (int i = 0; i < *n; i++)
        y[i * (*incy)] += alpha * x[i * (*incx)];
    return 0;
}

// toy_daxpy_mixed: n and incy by reference, incx by value -- an extent
// operand pair (X's n/incx) and a lone pointee operand (Y's incy) don't
// have to agree on sourcing; this exercises a spec mixing both within one
// call.
int toy_daxpy_mixed(const int *n, double alpha, const double *x, int incx,
                     double *y, const int *incy) {
    for (int i = 0; i < *n; i++)
        y[i * (*incy)] += alpha * x[i * incx];
    return 0;
}

// toy_daxpy_badindex: identical to toy_daxpy, registered under its own
// symbol purely so a test can pair it with a spec whose extent operand
// arg_index is deliberately out of range without colliding with
// toy_daxpy's own (correct) registration in the same grate.
int toy_daxpy_badindex(int n, double alpha, const double *x, int incx, double *y, int incy) {
    return toy_daxpy(n, alpha, x, incx, y, incy);
}

// toy_daxpy_exprstride: same computation as toy_daxpy, but walks forward
// using the ABSOLUTE VALUE of incx/incy -- unlike toy_daxpy's raw
// (possibly-negative) incx/incy. Used to test LIND_SIZE_STRIDE_VECTOR's
// general lind_extent_expr operands (size_operand_expr/stride_operand_expr
// -- issue #22's Gate 3, import_openblas_inference.py's own output shape
// for a real OpenBLAS "stride_vector" extent), which can wrap a stride in
// abs() the same way a real formula does for cblas_dger/cblas_dnrm2/etc.
// That lets a spec accept a negative incx/incy (rejected outright by the
// plain-lind_extent_operand path's own "negative stride not supported"
// check, since that path has no way to express "take the magnitude"). Real
// BLAS direction semantics for a negative increment (walking backward from
// the LAST touched element) are a separate, pre-existing limitation this
// toy function does not attempt to reproduce -- it only proves the general
// expression-tree sizing path itself, not full real-BLAS parity.
int toy_daxpy_exprstride(int n, double alpha, const double *x, int incx,
                          double *y, int incy) {
    int ix = incx < 0 ? -incx : incx;
    int iy = incy < 0 ? -incy : incy;
    for (int i = 0; i < n; i++)
        y[i * iy] += alpha * x[i * ix];
    return 0;
}

// toy_packedsum_exprsize: sums the n*(n+1)/2 doubles of a packed buffer --
// the real formula shape import_openblas_inference.py lowers for
// OpenBLAS's packed-storage ("sp"/"pp") functions
// (ceil_divide(product(n, add(n, 1)), 2), scaled by the element size).
// Used to test LIND_SIZE_EXPR (issue #22's Gate 3): a pointer's whole byte
// extent computed directly from a general lind_extent_expr tree, not any
// existing size_kind.
double toy_packedsum_exprsize(int n, const double *ap) {
    int count = (n * (n + 1)) / 2;
    double sum = 0.0;
    for (int i = 0; i < count; i++) sum += ap[i];
    return sum;
}

// toy_packedsum_badtree: registered under its own symbol purely so a test
// can pair it with a spec whose size_expr tree deliberately names an
// out-of-range argument index, without colliding with
// toy_packedsum_exprsize's own (correct) registration in the same grate.
// Returns int (unlike toy_packedsum_exprsize's double), the same reason
// toy_daxpy returns int instead of real cblas_daxpy's void: a rejected
// call's LIND_GRATE_ERR sentinel needs a return value to travel back in,
// and this function is never expected to actually run in the first place
// (the call is rejected before reaching it).
int toy_packedsum_badtree(int n, const double *ap) {
    (void)n; (void)ap;
    return 0;
}
