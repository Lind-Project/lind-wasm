// The entry function makes TWO SEPARATE relevant delegating calls to two
// DISTINCT, REAL (defined-in-this-module) callees, each receiving a
// traced pointer -- used to exercise function-count truncation
// (--llm-max-functions 2 admits the entry plus only ONE of the two
// callees, and must record the other as incomplete) without also
// triggering call-depth truncation. Both callees must have real bodies
// here (not mere declarations), or they would be reported as "no
// available body" instead of exercising the function-count limit.
__attribute__((noinline)) static void callee_a(double *x) { *x += 1.0; }
__attribute__((noinline)) static void callee_b(double *y) { *y += 2.0; }

void caller_two_callees(double *x, double *y) {
    callee_a(x);
    callee_b(y);
}
