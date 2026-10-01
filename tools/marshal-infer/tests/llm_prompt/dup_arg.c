// One caller argument passed to TWO DIFFERENT callee parameters at the
// same call site -- the argument_map for this edge must show the SAME
// caller argument under both callee_argument entries, which only an
// array-of-correspondences representation (not a JSON object keyed by
// argument) can express without ambiguity. The callee has a real body
// (not a mere declaration) so the call actually resolves and the
// argument_map is populated, rather than being reported as an unresolved
// "no available body" call.
__attribute__((noinline)) static void callee_dup(double *a, double *b) {
    *a += *b;
}

void caller_dup(double *x) {
    callee_dup(x, x);
}
