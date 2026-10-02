// Preloaded library stub for extentexpr_cage.c's toy_extent_probe import --
// register_lib_handler cannot fabricate a symbol out of nothing, only
// intercept a real one (see fail-registration's identical convention).
// This body must never actually run once interposed.
#include <stdint.h>
#include <stdio.h>

long long toy_extent_probe(int mode, int64_t a0, int64_t a1, uint32_t a2, int a3, int a4) {
    (void)mode; (void)a0; (void)a1; (void)a2; (void)a3; (void)a4;
    fprintf(stderr, "[libextentexpr_stub] FAIL: real (uninterposed) implementation ran\n");
    return -1;
}
