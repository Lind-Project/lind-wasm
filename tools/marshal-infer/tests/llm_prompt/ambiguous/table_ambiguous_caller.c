// A function-pointer table with one real entry and one entry
// ('table_ambig_shared') that resolves AMBIGUOUSLY once both
// table_ambig_workerA.bc and table_ambig_workerB.bc are resident (see
// their own comments) -- cross-module resolution must refuse to guess
// which body it means, exactly like an ordinary direct call to an
// ambiguous name would (see resolveDeclaration in LlmPrompt.cpp).
void table_ambig_shared(double *x);

__attribute__((noinline)) static void table_ambig_kernel_ok(double *x) { *x += 1.0; }

void caller_table_ambiguous(int which, double *x) {
    void (*table[])(double *) = { table_ambig_kernel_ok, table_ambig_shared };
    (table[which & 1])(x);
}
