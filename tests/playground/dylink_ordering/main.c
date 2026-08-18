#include <stdio.h>

/* main provides the only definition of xerbla_ in this reproducer, exported
 * with ordinary default visibility. do_work() (in the preloaded lib.so) calls
 * it internally via a genuine env::xerbla_ import -- there is no other
 * definition anywhere for that import to have raced against, so this isolates
 * pure load-order resolution (issue #1) from wasm-ld's local-symbol-shadowing
 * gap (issue #2).
 */
void xerbla_(char *msg) {
    printf("[main] override xerbla_: %s\n", msg);
}

extern void do_work(void);

int main(void) {
    do_work();
    return 0;
}
