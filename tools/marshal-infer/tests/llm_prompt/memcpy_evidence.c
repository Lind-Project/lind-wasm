// A bulk memory intrinsic reached by a traced pointer is COMPLETE evidence
// on its own -- the intrinsic call and its explicit byte-count operand are
// both directly visible in the included function's own IR, so this must
// be recorded as Informational only, never as an incompleteness (unlike an
// unresolved ordinary call, which has no operand revealing what it does).
void caller_memcpy(double *dst, const double *src, int n) {
    __builtin_memcpy(dst, src, (unsigned long)n * sizeof(double));
}
