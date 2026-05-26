// Cage app: calls strlen() on a known string.
// Under the libc-strlen grate, strlen is intercepted and returns len*2.
#include <stdio.h>
#include <string.h>
#include <assert.h>

// Global so the string lives in the data section (vmmap-accessible),
// not on the WASM virtual stack which copy_data_between_cages cannot reach.
static char g_s[] = "hello";

int main(void) {
    size_t r = strlen(g_s);
    printf("[Cage] strlen(\"%s\") = %zu\n", g_s, r);
    // Real strlen("hello") = 5. Grate handler returns 5 * 2 = 10.
    if (r != 10) {
        fprintf(stderr, "[Cage] FAIL: expected 10 (interposed), got %zu\n", r);
        assert(0);
    }

    printf("[Cage] PASS\n");
    return 0;
}
