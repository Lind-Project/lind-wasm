// Compiled WITHOUT -g in run.sh -- see nodebug_caller.c's own comment.
void nodebug_callee(int n, double *x) {
    for (int i = 0; i < n; i++) x[i] += 1.0;
}
