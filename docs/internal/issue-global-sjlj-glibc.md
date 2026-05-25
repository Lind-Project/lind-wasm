# Issue: Global `-fwasm-exceptions -mllvm -wasm-enable-sjlj` Cannot Be Applied to All of glibc

**Branch:** `setjmp-alt-impl`
**Investigated:** 2026-05-24
**Status:** Open — targeted workaround in use; root cause not fully resolved

---

## Background

The LLVM `WebAssemblyLowerEmscriptenEHSjLj` (SjLj) pass transforms every `_setjmp` call site
in a compilation unit into a `saveSetjmp` + `try_table` pair, and transforms every `longjmp`
call into a call to `__wasm_longjmp`.  It is activated by the combination:

```
-fwasm-exceptions -mllvm -wasm-enable-sjlj
```

This pass is already applied to `wasm_eh_setjmp.c` (which provides `__wasm_longjmp` itself)
and to `wasm_eh_c_longjmp_tag.c` (which seeds the `__c_longjmp` tag definition).  It is also
applied to all user code compiled by `lind-clang`.

### Why global application was desired

Two glibc-internal code paths call `__sigsetjmp` / `setjmp` and rely on the resulting
`longjmp` to unwind their stack:

| File | Call site | Purpose |
|------|-----------|---------|
| `elf/dl-catch.c` | `__sigsetjmp` in `_dl_catch_exception` | catches exceptions during dynamic loading |
| `sysdeps/nptl/pthread.h` | `__sigsetjmp_cancel` (pthread cleanup) | cancellation-point unwinding |

Without the SjLj pass, `longjmp` inside these functions is compiled as a plain call and does
nothing (in WebAssembly there is no way to "return" through caller frames via a plain call).
The EH-based `longjmp` used by all other code cannot be caught by the un-instrumented glibc
catch blocks, leading to either crashes or unhandled exceptions.

---

## What Was Tried

`GLIBC_SETJMP_CFLAGS` in `scripts/make_glibc_and_sysroot.sh` was changed from:

```bash
GLIBC_SETJMP_CFLAGS="-DLIND_EH_SETJMP"
```

to:

```bash
GLIBC_SETJMP_CFLAGS="-DLIND_EH_SETJMP -fwasm-exceptions -mllvm -wasm-enable-sjlj"
```

This injects the flags into the `../configure … CFLAGS=…` line, which propagates them to the
entire `make` build of glibc.

---

## Observed Failures

Running `make sysroot` with the global flags produced failures in two categories.

### Category 1: Pre-existing compilation errors (unrelated to SjLj pass)

Many glibc source files are not meant to be compiled standalone or in the lind-wasm build
context.  They fail with or without the SjLj flags.  Re-confirming this by compiling every
`.c` file in `libio/`, `nptl/`, `elf/`, and `posix/` individually with the lind-wasm CFLAGS
plus the SjLj flags gives:

| Directory | Pass | Fail | Notes on failures |
|-----------|------|------|-------------------|
| `libio/`  | 212  |  3   | `iovdprintf.c` (missing `errno`), `libc_fatal.c` (missing `abort` decl), `tst_putwc.c` (test-only, needs `OBJPFX`) |
| `nptl/`   | 286  |  31  | Missing generated headers (`nptl-stack.h`), test-only files, inline asm |
| `elf/`    | 575  | 102  | Missing generated headers, `.fini_array` backend error, inline asm directives unsupported on wasm32 |
| `posix/`  | 248  |  29  | Missing generated headers, test-only files |

None of these failures are caused by the SjLj pass itself.  They pre-exist and are suppressed
during a normal `make` run because those files are either not compiled, or compiled with
additional `-I` paths that are generated during the build.

### Category 2: LLVM backend crash (the blocking issue)

When the SjLj flags are applied via `CFLAGS` in `configure`, clang crashes during the `make`
run with exit code 133 (SIGTRAP — an LLVM internal trap in a release build, equivalent to an
assertion failure).  The crash occurs inside the `WebAssembly Instruction Selection` sub-pass
of the `Function Pass Manager`.

This crash was **confirmed reproducible** with the exact `-cc1` command extracted from the
`make` build log.

#### Minimal repro

From `src/glibc/libio/`:

```
clang-18 -cc1 -triple wasm32-unknown-wasi -emit-obj \
  -mrelocation-model pic -pic-level 2 -pic-is-pie \
  -target-feature +exception-handling -mllvm -wasm-enable-eh \
  -target-feature +exception-handling -exception-model=wasm \
  -target-feature +atomics -target-feature +bulk-memory \
  -fexceptions -exception-model=wasm \
  -mllvm -wasm-enable-sjlj \
  [full include/define flags] \
  -x c iofclose.c
```

#### Stack dump (clang 18.1.8, wasm32-unknown-wasi)

```
Stack dump:
0.  Program arguments: clang-18 -cc1 ... -mllvm -wasm-enable-sjlj ... iofclose.c
1.  <eof> parser at end of file
2.  Code generation
3.  Running pass 'Function Pass Manager' on module 'iofclose.c'.
4.  Running pass 'WebAssembly Instruction Selection' on function '@_IO_new_fclose'
#0 llvm::sys::PrintStackTrace(llvm::raw_ostream&, int)
#1 llvm::sys::RunSignalHandlers()
#2 SignalHandler(int)                    Signals.cpp:0:0
#3 libc.so.6  +0x42520
#4 clang-18   +0x7054659
Trace/breakpoint trap (core dumped)
exit: 133
```

The crash point is frame 4 inside the wasm backend code generator (`+0x7054659`), invoked
during instruction selection on `@_IO_new_fclose`.  The function body of `_IO_new_fclose`
contains indirect function pointer calls through the `_IO_jumps` vtable (e.g.,
`(*fp->_vtable_offset)(...)`), which are also call sites the SjLj pass must instrument.  The
hypothesis is that the SjLj pass generates malformed IR for one of these indirect call
patterns, which the wasm instruction selector cannot handle, trapping on an assertion.

**Why standalone compilation (without the full `make` header chain) doesn't crash:** the
`iofclose.c` standalone test earlier failed because it was run from `$BUILD` without the
correct `-fdebug-compilation-dir` and without the generated `libc-modules.h` state produced
by `make`.  When run with the exact same preprocessed state (`-cc1` passthrough), the crash
reproduces deterministically.

Affected libio files confirmed crashing during the `make` run include: `iofclose.c`,
`iofflush.c`, `iofgetpos.c`, `iofsetpos.c`, `ioseekpos.c`, `ioseekoff.c`, `iogetdelim.c`,
`ioftell.c`, `ioungetc.c`, `iofgets.c`, `iofread.c`, `iofputs.c`, `iofwrite.c`,
`getwc.c`, `putwchar.c`, `fputwc.c`, `iofgetws.c`, `iofputws.c`, `iosetbuffer.c`,
`putchar.c` — and others across `nptl/` and elsewhere (totalling approximately 123 objects
in the full parallel build).

---

## Root Cause Assessment

The SjLj pass was designed to be applied selectively to compilation units that **contain**
`setjmp`/`longjmp` call sites, not to an entire library of hundreds of files.  Applying it
globally:

1. Instruments every non-excluded call site in every function — including the indirect vtable
   dispatch calls in `libio` (`_IO_jumps` function pointers).  The pass appears to generate
   invalid IR for at least one of these call patterns, causing the wasm instruction selector
   to trap.
2. Is unnecessary for most of glibc: only files that actually call `_setjmp` /
   `__sigsetjmp` need the pass; the rest gain no benefit.
3. The crash is in the **wasm backend** (`WebAssembly Instruction Selection`), not in the
   SjLj pass itself — meaning the pass produces IR that passes LLVM IR verification but the
   wasm instruction selector cannot lower.  This makes it an LLVM 18 bug (likely
   unreported, as applying SjLj globally to a large C library is a non-standard use case).

---

## Affected glibc Code Paths (the actual need)

Only two files contain `setjmp`-family calls that must interoperate with EH-based `longjmp`
from user code:

### `elf/dl-catch.c` — `_dl_catch_exception`

```c
/* Inside _dl_catch_exception: */
int __sigsetjmp (struct __jmp_buf_tag *env, int savemask);
...
if (__sigsetjmp (buf, 0) == 0) {
    /* run the operation that may throw */
    ...
}
```

This is glibc's internal exception mechanism for dynamic loading.  It is currently **not
affected** in lind-wasm because lind implements its own `dlopen`/`dlclose` in Rust (in
`src/lind-boot`) and does not use `_dl_catch_exception` at all.  The `test_dlopen_error` test
in the previous test suite was testing lind's own error return, not this path.

### `sysdeps/nptl/pthread.h` — pthread cancellation via `__sigsetjmp_cancel`

```c
/* Cleanup handler push uses __sigsetjmp_cancel: */
# define pthread_cleanup_push(routine, arg) \
  do {                                       \
    __pthread_cleanup_class __clframe       \
      (routine, arg);
```

This macro eventually calls `__sigsetjmp_cancel`, and the cleanup handler invocation calls
the longjmp path.  This path **is** active in lind-wasm programs that use
`pthread_cleanup_push`.

---

## Current Status / Workaround

The global approach is reverted.  `GLIBC_SETJMP_CFLAGS` is back to `-DLIND_EH_SETJMP` only.

`elf/dl-catch.c` does not need fixing (lind doesn't use it).

The pthread cleanup path (`sysdeps/nptl/pthread.h`) is potentially affected but has not
yet been verified to be broken in practice.

---

## Recommended Fix

Instead of modifying `GLIBC_SETJMP_CFLAGS` globally, recompile only the specific object
files that need the SjLj pass, after the main `make` step, and replace their entries in the
archive.

Add to `scripts/make_glibc_and_sysroot.sh`, after the `make -j…` line:

```bash
# Recompile specific glibc files that contain setjmp call sites and must
# interoperate with EH-based longjmp.  These are compiled separately because
# the SjLj pass cannot be applied globally without triggering LLVM crashes.
SJLJ_FLAGS="-fwasm-exceptions -mllvm -wasm-enable-sjlj"

# pthread cleanup via __sigsetjmp_cancel (nptl/pthread_cancel.c or similar)
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    $SJLJ_FLAGS \
    -o $BUILD/nptl/pthread_cleanup_target.o \
    -c $GLIBC/nptl/pthread_cleanup_target.c

# Replace in archive
llvm-ar r $BUILD/libc.a $BUILD/nptl/pthread_cleanup_target.o
```

The exact files were confirmed by grep:

```
$ grep -r '\b_setjmp\b\|__sigsetjmp\b\|__sigsetjmp_cancel\b' \
    src/glibc/nptl/ src/glibc/elf/ src/glibc/sysdeps/nptl/ \
    --include='*.c' --include='*.h' -l

src/glibc/elf/dl-catch.c
src/glibc/sysdeps/nptl/pthread.h
```

`dl-catch.c` is the only `.c` file; `pthread.h` is a header with an inline that expands into
`.c` files that include it (primarily `nptl/` cancellation sources).

---

## References

- LLVM pass: `llvm/lib/Target/WebAssembly/WebAssemblyLowerEmscriptenEHSjLj.cpp`
- Related: `docs/internal/setjmp-eh-impl.md` — full EH setjmp implementation notes
- Related: `docs/internal/setjmp-remaining-tasks.md` — other open items on this branch
