// Gate 0 (cross-cage function-pointer callbacks) feasibility probe: cage A.
// Passes its own function pointer into an interposed library call and
// expects the grate to call back into THIS instance before the outer call
// returns -- see local-notes/active/plan-cross-cage-function-pointers.md,
// Gate 0's exact scenario.
//
// Compiled statically (-s) with the indirect-function table exported, so
// the host can resolve this cage's own table entries by export name. Gate 0
// is scoped to proving the execution topology, not generalizing across
// every build mode -- a dynamically-linked cage's imported (not locally
// exported) table is deferred along with the rest of this probe's
// deliberately narrow scope.
//
// Compile:
//   lind-clang -s callback_cage.c -- -Wl,--export-table
#include <stdio.h>

static int observed = 0;

static void callback(int value) {
    observed = value;
}

extern void library_call(void (*callback)(int));

int main(void) {
    library_call(callback);
    if (observed != 42) {
        printf("[Cage|gate0-callback] FAIL: observed=%d expected 42\n", observed);
        return 1;
    }
    printf("[Cage|gate0-callback] PASS: callback executed, observed=%d\n", observed);
    return 0;
}
