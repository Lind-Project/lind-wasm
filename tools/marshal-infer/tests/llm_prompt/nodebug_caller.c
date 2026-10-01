// Paired with nodebug_callee.c, compiled WITHOUT -g in run.sh specifically
// to prove debug info is a label, never a requirement: the callee must
// still be followed and included, labeled with plain argN identities,
// not rejected or skipped for lacking DWARF.
void nodebug_callee(int n, double *x);

void nodebug_caller(int n, double *x) {
    nodebug_callee(n, x);
}
