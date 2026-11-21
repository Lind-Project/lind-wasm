/*
 * Shared library for cross-module longjmp test.
 *
 * lib_do_longjmp is called from main-module user code.  The longjmp here is
 * lowered by the LLVM SjLj pass to __wasm_longjmp (in libc.so), which throws
 * the __c_longjmp exception tag.  The throw crosses two module boundaries
 * (this library → libc.so → user code) and is caught by the try_table that
 * the SjLj pass inserted at the setjmp call site in user code.
 */

#include <setjmp.h>

void lib_do_longjmp(jmp_buf *buf, int val)
{
    longjmp(*buf, val);
}
