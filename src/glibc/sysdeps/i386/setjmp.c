#include <stddef.h>
#include <setjmp.h>
#include <signal.h>

/* In EH mode the LLVM WebAssemblyLowerEmscriptenEHSjLj pass transforms every
   sigsetjmp/__sigsetjmp call site in user code: it inserts saveSetjmp() before
   the call and wraps the continuation in a try/catch for __c_longjmp.
   __sigsetjmp therefore only saves the signal mask and returns 0; the actual
   setjmp state is managed by the LLVM-generated wrappers at the call site.
   siglongjmp → __libc_siglongjmp → __longjmp → __wasm_longjmp throws
   __c_longjmp through pause()/signal_callback (pure wasm, no Rust boundary)
   up to the call-site try/catch. */
int __sigsetjmp (jmp_buf env, int savemask) {
    env[0].__mask_was_saved = (savemask
        && sigprocmask(SIG_BLOCK, (sigset_t *)NULL, (sigset_t *)&env[0].__saved_mask) == 0);
    return 0;
}

