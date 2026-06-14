/*
 * Defines the __c_longjmp wasm exception tag weakly so that programs which
 * do not call setjmp themselves can still link against libc.a.
 *
 * Background: __wasm_longjmp (in wasm_eh_setjmp.o) uses
 * __builtin_wasm_throw(1, ...) which references env.__c_longjmp as a tag
 * import.  In binaries where no user code calls setjmp, no object provides
 * a definition of __c_longjmp, causing wasm-ld to report an undefined symbol.
 *
 * The LLVM WebAssemblyLowerEmscriptenEHSjLj pass defines __c_longjmp weakly
 * (in the Tag section) in any object that has a _setjmp call site followed
 * by at least one non-excluded function call.  This object provides exactly
 * that anchor so the linker always has a weak definition available.
 *
 * Key constraints:
 *   - Must be compiled WITHOUT -fPIC/-fPIE: those flags cause the pass to
 *     emit an import rather than a local weak Tag definition.
 *   - The post-setjmp call must NOT be setTempRet0/getTempRet0/saveSetjmp/
 *     testSetjmp (the pass excludes its own helpers).  We use __libc_write
 *     (from libc itself, always available in the link) as a harmless stub.
 */

static int __c_longjmp_tag_buf[8];

extern int _setjmp(int *);
/* Any non-SjLj-excluded function call after _setjmp triggers the pass. */
extern long __libc_write(int, const void *, unsigned long);

__attribute__((used, noinline)) static void __define_c_longjmp_tag(void)
{
    if (_setjmp(__c_longjmp_tag_buf) == 0)
        __libc_write(0, 0, 0);  /* never actually called */
}
