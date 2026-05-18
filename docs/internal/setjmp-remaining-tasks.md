# Wasm EH setjmp/longjmp — Remaining Tasks

Branch: `setjmp-alt-impl`

## Background

The core implementation is working for both static (`lind-clang -s`) and dynamic builds.
The clang 18 wasm-sjlj ABI is used: `saveSetjmp`/`testSetjmp`/`__wasm_longjmp` helpers in
`src/glibc/setjmp/wasm_eh_setjmp.c`, compiled into `libc.so` and preloaded via lind-boot.

---

## 1. `sigsetjmp` / `siglongjmp` (not implemented)

The POSIX signal-mask-saving variants are not yet implemented.

- `sigsetjmp(buf, savemask)` should save the current signal mask into `buf` when `savemask != 0`.
- `siglongjmp(buf, val)` should restore the signal mask saved by `sigsetjmp` before jumping.

This requires calling `sigprocmask` (or the lind-wasm equivalent) during save and restore.
Programs that use `sigsetjmp`/`siglongjmp` (e.g. for signal-safe longjmp) will silently
misbehave without this — the jump itself will work but the signal mask will not be restored.

**Suggested approach:** wrap the existing EH-based setjmp/longjmp in `sigsetjmp`/`siglongjmp`
stubs that save/restore the signal mask around the jump.

---

## 2. Table memory leak on normal function exit

`saveSetjmp` allocates a per-call-site table via `malloc` (initial size 4 entries, 40 bytes,
grows via `realloc` if needed). The table is freed by the EH catch block when a longjmp is
caught (either the matching frame or a re-thrown one). However, if the protected scope exits
*normally* (no longjmp ever occurs), the table is never freed.

This is a latent per-`setjmp`-call-site memory leak proportional to the number of times the
enclosing function is called without a corresponding longjmp. It is visible in the performance
benchmark: `bench_setjmp_only` (setjmp with no longjmp, 1000 iterations) runs ~70× slower
than the roundtrip benchmarks because the allocator is absorbing 1000 unreleased 40-byte
tables.

**Suggested approach:** emit a cleanup call to `free(table)` on the normal exit path. This
requires either a compiler-side change (clang's wasm-sjlj lowering) or a wrapper in
`wasm_eh_setjmp.c` that intercepts the normal-exit case.

---

## 3. Dead asyncify setjmp infrastructure (cleanup)

Before the EH-based implementation, setjmp/longjmp used asyncify stack snapshots stored in
the wasmtime store. That mechanism was replaced but never removed. The following dead code
remains in `src/wasmtime/crates/wasmtime/src/runtime/store.rs`:

- `stack_snapshots: HashMap<u64, Vec<u8>>` field and its initialization in two `new()` sites
- `store_unwind_data()` — captures raw asyncify stack state into the map
- `retrieve_unwind_data()` — looks up a snapshot by hash
- `get_stack_snapshots()` / `set_stack_snapshots()` — used by fork to copy the map to the
  child (the copy-on-fork path is still live, but the map is always empty now)

No caller in `lind-boot/src/` invokes `store_unwind_data` or `retrieve_unwind_data` anymore.

**Action:** remove all four methods and the `stack_snapshots` field. Remove the
`set_stack_snapshots` call in the fork path (or keep it as a no-op if the field is retained
for future use).

---

## 4. ~~Regression testing~~ ✓ Done

The GC heap allocation fix (`Mmap::accessible_reserved(request_bytes, request_bytes)`) was
verified against the full test suite — all unit tests pass with no regressions.

---

## 5. ~~Edge-case tests~~ ✓ Done

`tests/playground/test_setjmp_edge.c` covers: nested setjmp buffers, deep call stack,
multiple longjmps to the same buffer, `longjmp(buf,0)` delivers 1, `longjmp(buf,1)` delivers
1 unchanged, longjmp via function pointer, re-throw propagation. Result: 9/9 passed.
