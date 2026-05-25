# PR: EH-based setjmp/longjmp, memory management refactor, pause/sigsuspend

## Overview

This PR contains three closely related changes that were developed together because each one was required to make the others work correctly.

### 1. EH-based setjmp/longjmp

Replaces the previous asyncify-based setjmp/longjmp with an implementation built on wasm exception handling. The new default uses `-fwasm-exceptions -mllvm -wasm-enable-sjlj` (clang 18) together with the `saveSetjmp`/`testSetjmp`/`__wasm_longjmp` runtime in `src/glibc/setjmp/wasm_eh_setjmp.c`. The old asyncify path is preserved and still selectable via `LIND_ASYNCIFY_SETJMP=1`.

For full implementation details, design decisions, known limitations, and open tasks see [`docs/internal/setjmp.md`](docs/internal/setjmp.md).

### 2. Linear memory management refactor

The EH implementation exposed a latent bug: wasmtime allocates a GC heap for wasm exception objects using `Mmap::reserve()` followed by `make_accessible()`. In lind-wasm, `make_accessible` had been made a no-op (to prevent wasmtime from mprotecting wasm linear memory), so the GC heap remained `PROT_NONE` and the first `longjmp` caused a SIGSEGV inside JIT-compiled code.

The fix clarifies ownership of `mprotect`:
- `Mmap::make_accessible` is restored to call real `mprotect`. It is correct for host-internal allocations (GC heap, code memory, etc.) that wasmtime fully owns.
- PROT_NONE enforcement for wasm *linear* memory is moved to `attach_shared_memory` (called once per cage, before `init_vmmap`), and to the `ClonedMemory::New` arm in `new_child_linker` (fork children). Rawposix vmmap then takes ownership of those pages in a clean PROT_NONE state.

### 3. `pause` syscall and `sigsuspend` fix

A key test for EH setjmp is longjmp from a signal handler: the handler throws `__c_longjmp` through `signal_callback` and `pause()` with no Rust boundary in the unwind path. This required:
- Implementing the `pause` syscall in rawposix.
- Fixing `sigsuspend` to set the signal mask before blocking, so a signal queued before the call is delivered correctly.

These syscalls are also independently useful; the setjmp tests just exposed that they were missing or incomplete.

## Tests

- `tests/unit-tests/process_tests/deterministic/setjmp_edge.c` — comprehensive single-module edge cases
- `tests/unit-tests/process_tests/deterministic/test_crossmodule_longjmp.c` — cross-module EH tag sharing and signal path
- `tests/unit-tests/signal_tests/deterministic/test_sigsetjmp.c` — sigsetjmp/siglongjmp API
- `tests/unit-tests/dylink_tests/deterministic/longjmp_dlopen.c` — dlopen cross-module (skipped in main runner)
- `tests/unit-tests/memory_tests/fail/invalid_access_direct.c` — PROT_NONE enforcement (direct)
- `tests/unit-tests/memory_tests/fail/invalid_access_fork.c` — PROT_NONE enforcement (fork child)
