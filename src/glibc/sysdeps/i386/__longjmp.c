#include <setjmp.h>

#ifdef LIND_EH_SETJMP

/* EH-based setjmp mode: throw a __c_longjmp wasm exception via __wasm_longjmp
   (defined in wasm_eh_setjmp.c, compiled with -fwasm-exceptions).
   env points to __jmpbuf[0] inside struct __jmp_buf_tag; the surrounding
   struct is large enough for the four int slots __wasm_longjmp needs. */
extern void __wasm_longjmp (int *buf, int val) __attribute__ ((__noreturn__));

void __longjmp (__jmp_buf env, int val)
{
    __wasm_longjmp ((int *) env, val == 0 ? 1 : val);
}

#else /* asyncify-based setjmp mode */

int __imported_wasi_lind_longjmp(unsigned int jmp_buf, unsigned int retval) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-longjmp")
));

void __longjmp (__jmp_buf env, int val)
{
    __imported_wasi_lind_longjmp ((unsigned int) env, (unsigned int) val);
}

#endif
