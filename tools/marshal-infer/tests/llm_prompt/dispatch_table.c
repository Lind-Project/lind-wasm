// A local array of function pointers, every entry a compile-time-constant
// reference to a real, in-module function -- the "int (*table[])(...) =
// {A, B}; ...; table[idx](...)" dispatch idiom real libraries (OpenBLAS's
// level-2/level-3 BLAS drivers, among others) use to pick a specialized
// kernel by a small flag. Clang hoists such a table to a private constant
// global regardless of optimization level, since every element is a
// constant function address; resolveFunctionPointerTable (LlmPrompt.cpp)
// recognizes that shape and treats every entry as a candidate callee.
// Contrast wrapper_indirect.c (stride_vector_extent/indirect_callee.c),
// where the call target is an opaque function-pointer PARAMETER with no
// enumerable candidate set -- that case must stay unresolved.
__attribute__((noinline)) static void kernel_a(double *x) { *x += 1.0; }
__attribute__((noinline)) static void kernel_b(double *x) { *x += 2.0; }

void caller_dispatch_table(int which, double *x) {
    void (*table[])(double *) = { kernel_a, kernel_b };
    (table[which & 1])(x);
}
