// A function-pointer table with one real, defined entry and one entry
// ('table_missing_worker') that is only DECLARED, never defined anywhere in
// this test's compiled set. Cross-module resolution must fail for that
// entry exactly the way it would for an ordinary direct call to an
// undefined function (see resolveDeclaration in LlmPrompt.cpp) -- it must
// not be silently skipped or treated as resolved. The other entry still
// resolves and is explored.
void table_missing_worker(double *x);

__attribute__((noinline)) static void table_kernel_ok(double *x) { *x += 1.0; }

void caller_table_declaration_only(int which, double *x) {
    void (*table[])(double *) = { table_kernel_ok, table_missing_worker };
    (table[which & 1])(x);
}
