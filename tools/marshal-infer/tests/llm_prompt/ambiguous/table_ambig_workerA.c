// One of two conflicting externally-linked definitions of the same name
// referenced from a function-pointer table (see table_ambig_workerB.c and
// table_ambiguous_caller.c) -- e.g. what several architecture-specific TUs
// compiled into the same analysis run would look like, only one of which
// would actually be linked into a real binary.
void table_ambig_shared(double *x) { *x += 10.0; }
