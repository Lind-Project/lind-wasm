// A pointer traced from an argument is stored to memory (a global here) --
// the analysis does not follow a stored pointer to wherever it might be
// read back later, so this must be recorded as an Incomplete note and
// slice_complete must be false, never silently treated as fully resolved.
double *g_saved;

void caller_store(double *x) {
    g_saved = x;
}
