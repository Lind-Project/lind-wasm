// Preloaded stub exporting the REAL implementations of this test's two
// toy functions (issue #22's OpenBLAS inference-to-runtime integration,
// Gate 5: "strict execution oracle"). Both are deliberately correct
// regardless of whether a call is interposed: that's the whole point --
// a numerically correct result alone can never prove a real dispatch
// happened, since a silent fallback to THIS real, uninterposed
// implementation produces the identical answer. Only
// lind_marshal.h's own debug execution trace (a grate built with
// -DLIND_MARSHAL_DEBUG) can tell the two apart -- see exectrace_grate.c/
// exectrace_cage.c for how this test proves that.
int toy_trace_add(int a, int b) {
    return a + b;
}

int toy_trace_sum(int n, const int *x) {
    int s = 0;
    for (int i = 0; i < n; i++) s += x[i];
    return s;
}
