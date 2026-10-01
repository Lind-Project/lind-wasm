// The conflicting definition -- see table_ambig_workerA.c. A DIFFERENT body
// under the SAME externally-linked name; which one a cross-module reference
// to table_ambig_shared actually resolves to isn't knowable from IR alone.
void table_ambig_shared(double *x) { *x -= 10.0; }
