// Two DIFFERENT caller arguments merged (via a select, the common lowering
// of a pointer-valued ternary at -O1+) into ONE callee parameter -- the
// argument_map for this edge must list BOTH caller arguments under the
// SAME callee_argument entry, which only a list-valued correspondence
// (not a single scalar mapping) can express. The callee has a real body
// so the call actually resolves.
__attribute__((noinline)) static void callee_merge(double *p) {
    *p += 1.0;
}

void caller_merge(int cond, double *a, double *b) {
    double *p = cond ? a : b;
    callee_merge(p);
}
