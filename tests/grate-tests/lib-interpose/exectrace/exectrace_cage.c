// Cage for Gate 5's own self-test (issue #22's OpenBLAS inference-to-
// runtime integration): calls both toy functions and checks the
// numerically correct result -- true whether or not exectrace_grate.c
// actually registered a handler for either one, since exectrace_stub.c's
// own real implementation is correct either way. The POINT of this test
// is that PASS/FAIL alone can never distinguish a real dispatch from a
// local fallback; see run_tests.sh's own invocation for how
// tools/marshal-gen/execution_oracle.py's classify() uses THIS numeric
// result together with the combined output's [lind-trace] evidence (or
// lack of it) to tell PASS_INTERPOSED apart from PASS_LOCAL_ONLY.
#include <stdio.h>

extern int toy_trace_add(int a, int b);
extern int toy_trace_sum(int n, const int *x);

int main(void) {
    int rc = 0;

    int add_got = toy_trace_add(3, 4);
    if (add_got != 7) {
        printf("[Cage|exectrace] FAIL: toy_trace_add got=%d want=7\n", add_got);
        rc = 1;
    } else {
        printf("[Cage|exectrace] PASS: toy_trace_add\n");
    }

    int x[5] = { 1, 2, 3, 4, 5 };
    int sum_got = toy_trace_sum(5, x);
    if (sum_got != 15) {
        printf("[Cage|exectrace] FAIL: toy_trace_sum got=%d want=15\n", sum_got);
        rc = 1;
    } else {
        printf("[Cage|exectrace] PASS: toy_trace_sum\n");
    }

    return rc;
}
