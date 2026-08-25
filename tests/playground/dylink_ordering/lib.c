/* Intentionally NO definition of xerbla_ here.
 *
 * This is the key difference from tests/playground/dylink_min/: there, lib.c
 * defined its own xerbla_, so wasm-ld resolved do_work()'s call to a direct,
 * already-linked wasm-local function reference at lib.c's own build time --
 * it never became a wasm import at all (the wasm-ld visibility gap, issue #2).
 *
 * Here, xerbla_ is only declared (extern), never defined in this file, so
 * do_work()'s call to it *must* become a genuine `env::xerbla_` wasm function
 * import that the loader has to satisfy -- exercising the cross-module
 * symbol-resolution-order issue (issue #1) in isolation.
 */
extern void xerbla_(char *msg);

void do_work(void) {
    xerbla_("error");
}
