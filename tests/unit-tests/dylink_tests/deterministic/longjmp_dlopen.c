/*
 * Cross-module longjmp test via dlopen.
 *
 * Verifies that __c_longjmp tag identity is shared across all module
 * instances in the same Store.  longjmp_lib.cwasm calls longjmp(), which
 * is lowered to __wasm_longjmp in libc.so.  The resulting __c_longjmp
 * throw crosses two module boundaries (longjmp_lib → libc.so → user code)
 * and must be caught by the try_table at the setjmp call site here.
 *
 * This test requires a dynamic (dylink) build and dlopen support.
 */

#include <dlfcn.h>
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>

int main(void)
{
    void *h = dlopen("longjmp_lib.so", RTLD_LAZY);
    if (h == NULL) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }

    void (*lib_do_longjmp)(jmp_buf *, int) =
        (void (*)(jmp_buf *, int)) dlsym(h, "lib_do_longjmp");
    if (lib_do_longjmp == NULL) {
        fprintf(stderr, "dlsym failed: %s\n", dlerror());
        dlclose(h);
        return 1;
    }

    jmp_buf buf;
    int v = setjmp(buf);
    if (v == 0) {
        lib_do_longjmp(&buf, 55);
        fprintf(stderr, "FAIL: should not reach after lib_do_longjmp\n");
        dlclose(h);
        return 1;
    }

    dlclose(h);

    if (v != 55) {
        fprintf(stderr, "FAIL: expected 55, got %d\n", v);
        return 1;
    }
    printf("PASS: dlopen cross-module longjmp delivered %d\n", v);
    return 0;
}
