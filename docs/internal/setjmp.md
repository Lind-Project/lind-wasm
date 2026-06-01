# setjmp / longjmp in lind-wasm

## Two modes

Lind-wasm supports two setjmp/longjmp implementations, selected at build time
via the `LIND_ASYNCIFY_SETJMP` environment variable:

| Mode | How to select | Mechanism |
|------|---------------|-----------|
| **EH** (default) | unset `LIND_ASYNCIFY_SETJMP` | Wasm exception tags (`-fwasm-exceptions`), `__builtin_wasm_throw` |
| **Asyncify** | `LIND_ASYNCIFY_SETJMP=1` | `lind.lind-longjmp` host import, asyncify stack snapshots |

The EH mode is the default and recommended path. Asyncify mode exists as a
fallback for debugging or environments where wasm EH is unavailable.

### What `LIND_ASYNCIFY_SETJMP=1` changes

Setting this flag affects every layer of the build:

| Layer | EH (default) | Asyncify (`LIND_ASYNCIFY_SETJMP=1`) |
|-------|-------------|--------------------------------------|
| `make_glibc_and_sysroot.sh` | Compiles glibc with `-DLIND_EH_SETJMP`, `-fwasm-exceptions -mllvm -wasm-enable-sjlj`; compiles `wasm_eh_setjmp.o` and `wasm_eh_c_longjmp_tag.o` | Skips EH setjmp objects; `__longjmp` calls `lind.lind-longjmp` import |
| `lind_compile` (user programs) | Adds `-fwasm-exceptions -mllvm -wasm-enable-sjlj` to every clang invocation | No EH flags; asyncify handles longjmp via `lind.lind-longjmp` |
| `lind-wasm-opt` (binaryen) | Adds `--translate-to-exnref` after asyncify; passes `--enable-exception-handling --enable-reference-types` | Skips `--translate-to-exnref`; no EH feature flags |
| `make_shared_glibc.sh` | Strips `wasm_eh_c_longjmp_tag.o` from shared `libc.a` before linking `libc.so` | No special handling |
| `lind-boot` (Rust) | Built with `asyncify-setjmp` Cargo feature **absent**; defines `__c_longjmp` tag in linker | Built with `asyncify-setjmp` Cargo feature; no tag defined |

Usage:
```bash
# EH mode (default)
make

# Asyncify mode
LIND_ASYNCIFY_SETJMP=1 make
```

---

## Core mechanism: the LLVM SjLj pass (EH mode)

When user code is compiled with `-fwasm-exceptions -mllvm -wasm-enable-sjlj`,
the LLVM `WebAssemblyLowerEmscriptenEHSjLj` pass rewrites every `_setjmp`
call site:

```c
// User writes:
val = setjmp(buf);
...

// LLVM rewrites (conceptually):
table = saveSetjmp(buf, label_id, table, size);
size  = getTempRet0();
try {
    ...continuation...
} catch (__c_longjmp) {
    if (testSetjmp(buf[0], table, size)) {
        val = buf[3];   // normalised return value
        goto after_setjmp;
    } else {
        rethrow;        // belongs to an outer frame
    }
}
```

And `longjmp(buf, val)` is lowered to a call to `__wasm_longjmp(buf, val)`,
which throws the `__c_longjmp` wasm exception.

The runtime support functions are in `src/glibc/setjmp/wasm_eh_setjmp.c`:

- **`saveSetjmp(buf, label_id, table, size)`** — stores `(buf, label_id)` in
  a per-call-site table; grows via `realloc` when full; returns the new size
  via `setTempRet0`.
- **`testSetjmp(env, table, size)`** — searches the table for a matching `buf`
  address; returns the `label_id` of the matching frame, or 0 (must rethrow).
- **`getTempRet0` / `setTempRet0`** — thread-local "second return value" used
  because wasm imported functions cannot return multiple values.
- **`__wasm_longjmp(buf, val)`** — stores the normalised return value in
  `buf[3]`, a self-pointer in `buf[2]`, then throws `__c_longjmp` via
  `__builtin_wasm_throw(1, &buf[2])`.

### `jmp_buf` layout

`jmp_buf` is `long int[8]` (32 bytes on wasm32). The EH implementation uses
four 4-byte slots:

| Slot | Set by | Purpose |
|------|--------|---------|
| `buf[0]` | `saveSetjmp` | self-pointer for frame matching in `testSetjmp` |
| `buf[1]` | — | unused / reserved |
| `buf[2]` | `__wasm_longjmp` | self-pointer passed as exception payload |
| `buf[3]` | `__wasm_longjmp` | normalised return value (`val != 0 ? val : 1`) |

The normalisation rule: only `val == 0` is replaced with `1` (POSIX
requirement). Negative values and all other non-zero values pass through
unchanged.

### `__longjmp` dispatch

`src/glibc/sysdeps/i386/__longjmp.c` selects the implementation at compile time:

```c
#ifdef LIND_EH_SETJMP
void __longjmp(__jmp_buf env, int val) {
    __wasm_longjmp((int *)env, val == 0 ? 1 : val);
}
#else
void __longjmp(__jmp_buf env, int val) {
    __imported_wasi_lind_longjmp((unsigned int)env, (unsigned int)val);
}
#endif
```

---

## `sigsetjmp` / `siglongjmp`

### Why `sigsetjmp` must expand at the call site

The LLVM SjLj pass only instruments `_setjmp` call sites visible in the
compilation unit. If `sigsetjmp` remained a regular function call to
`__sigsetjmp`, the pass would emit no `try_table` at that call site — a
subsequent `longjmp` would throw `__c_longjmp` with nowhere to land.

The fix: `sigsetjmp` is a **macro** that expands to `_setjmp` directly in
user code, where the pass can see and transform it:

```c
// src/glibc/setjmp/setjmp.h
#define sigsetjmp(env, savemask)                                        \
  (__extension__ ({                                                      \
    int __sm = (savemask);                                              \
    (env)[0].__mask_was_saved =                                         \
      __sm && (sigprocmask(SIG_BLOCK, (__sigset_t *)NULL,               \
                           (__sigset_t *)&(env)[0].__saved_mask) == 0); \
    _setjmp(env);   /* LLVM transforms this */                          \
  }))
```

The signal mask is saved inline before `_setjmp`; restoration on `siglongjmp`
is handled by `__libc_siglongjmp` → `__longjmp` → `__wasm_longjmp` as normal.

### `sigprocmask` forward declaration

The macro calls `sigprocmask` before `<signal.h>` may have been included.
A forward declaration is added inside `<setjmp.h>`, guarded by
`#ifndef _SIGNAL_H` to prevent a type conflict when glibc includes
`<signal.h>` first (`sigset_t` vs `__sigset_t`).

`sigprocmask` (the public POSIX name) is used rather than the private
`__sigprocmask` so that dynamic builds resolve it from `libc.so`'s exported
symbol table.

---

## Cross-module tag sharing (dynamic builds)

In lind-wasm's dynamic build, `libc.so` and user code are separate wasmtime
`Instance` objects. Wasm exception tags are instance-level constructs: each
instance gets its own runtime tag value even if both name it `__c_longjmp`.
A throw using instance A's tag is not caught by instance B's catch.

**Solution:** the wasmtime host creates **one** `Tag` object per cage `Store`
and registers it in the `Linker` under `"env"."__c_longjmp"`:

```rust
// src/lind-boot/src/lind_wasmtime/execute.rs
let tag_type = TagType::new(FuncType::new(&engine, [ValType::I32], []));
let tag = Tag::new(&mut *wstore, &tag_type)?;
linker_guard.define(&*wstore, "env", "__c_longjmp", tag)?;
```

The same is done for forked children in `linker.rs:new_child_linker`.

When all module instances — user code, `libc.so`, any dynamically loaded
library — import `"env"."__c_longjmp"`, they all receive the same `Tag`
object. A throw in `libc.so` is caught by a `try_table` in user code.

All glibc objects are compiled with `-fPIC`. In PIC mode, the LLVM SjLj pass
emits a tag **import** for `__c_longjmp` rather than a local weak definition
— which is correct: the host owns the authoritative definition.

For the **static build** there is no host linker. Because `__builtin_wasm_throw(1, …)`
references `__c_longjmp` but the LLVM SjLj pass only emits a weak tag definition
in objects that contain a `_setjmp` call site, programs that never call `setjmp`
themselves would have an undefined `__c_longjmp` at link time.
`src/glibc/setjmp/wasm_eh_c_longjmp_tag.c` solves this with a synthetic object
that has a dummy `_setjmp` call site; the pass emits a weak `__c_longjmp` tag
definition into it, which `wasm-ld` uses to satisfy all imports. This object must
be compiled **without `-fPIE`** (`EXTRA_FLAGS_NO_PIE` in
`scripts/make_glibc_and_sysroot.sh`), and is excluded from the `libc.so` shared
link since the dynamic build uses host-provided tag imports instead.

---

## Longjmp from signal handlers

When a signal handler calls `longjmp`, the `__c_longjmp` exception must unwind
back through the signal delivery call chain — `signal_callback` → `pause()` /
`sigsuspend()` — to the `try_table` at the original `setjmp` call site. That
chain is entirely pure-wasm, so exception propagation works naturally. The one
place that requires explicit handling is the Rust/wasm boundary at
`signal_func.call()` in `signal.rs`.

`signal_func.call()` returns `Err(ThrownException)` when the wasm signal handler
throws an uncaught exception. For this error to re-enter wasmtime as a live wasm
exception (rather than a fatal cage error), `signal_handler` must return
`wasmtime::Result<i32>` — not `anyhow::Result<i32>`. `wasmtime::Result` preserves
`ThrownException` as a distinct variant; `?` propagates it through `epoch_callback`
back into the wasmtime execution engine, which re-throws the pending wasm exception
in the original execution context. It then propagates normally to the `try_table`
at the `setjmp` call site.

If `anyhow::Result` were used instead, `ThrownException` would be erased into a
generic error, the exception would not be re-thrown, and the cage would terminate
rather than returning to the setjmp site.

Note: any host function registered with `func_wrap` that may surface wasm
exceptions must follow the same rule — return `wasmtime::Result<T>`.

---

## Clang 18 and `--translate-to-exnref`

Wasm EH has two wire formats:

| Format | Instructions | Status |
|--------|-------------|--------|
| **Legacy** | `try` / `catch` / `rethrow` with inline tag indices | Emitted by clang 18 |
| **Standard (exnref)** | `try_table` / `exnref` as first-class values | W3C ratified spec; required by wasmtime's Cranelift backend |

Clang 18 can only emit the legacy format. Wasmtime's Cranelift only accepts the
standard exnref format. The gap is bridged by Binaryen's `--translate-to-exnref`
pass, which `lind-wasm-opt` runs after asyncify:

```bash
# from scripts/lind-wasm-opt
# Convert clang 18 legacy EH to standard EH after asyncify.
# Asyncify handles legacy EH natively; Cranelift only supports standard EH.
# --enable-reference-types is required because --translate-to-exnref emits
# (ref exn) blocks via catch_all_ref when setjmp frames are nested.
[[ -z "${LIND_ASYNCIFY_SETJMP:-}" ]] && LIND_FLAGS+=(--translate-to-exnref)
```

**This is a toolchain version limitation, not a design choice.** Clang 19+
can emit exnref natively, at which point `--translate-to-exnref` could be
dropped. Until the toolchain is upgraded, the pass is mandatory for all EH
mode builds.

---

## Comparison with wasi-libc / migration path

`wasm_eh_setjmp.c` was written from scratch for lind-wasm. It implements the
**old Emscripten JS-sjlj ABI** generated by clang 18. wasi-libc's `rt.c`
implements a **different, newer ABI** introduced in LLVM 19 (PR #84137). They
are not variants of the same design — they target two different compiler ABIs.

| Aspect | lind-wasm (`wasm_eh_setjmp.c`) | wasi-libc (`rt.c`, LLVM 19+) |
|--------|-------------------------------|-------------------------------|
| Entry points | `saveSetjmp(buf, label_id, table, size)` / `testSetjmp(env, table, size)` | `__wasm_setjmp(env, label, func_invocation_id)` / `__wasm_setjmp_test(env, func_invocation_id)` |
| Frame identity | per-thread malloc'd table of `(buf_ptr, label_id)` pairs | `func_invocation_id` pointer stored directly in `jmp_buf` (points to a local variable in the setjmp frame, unique per invocation) |
| `getTempRet0`/`setTempRet0` | C thread-local `g_tempRet0` | wasm global via `.globaltype tempRet0, i32` |
| `__wasm_longjmp` payload | throws `&buf[2]` (i32 self-pointer into `jmp_buf`) | throws `&jb->arg` (pointer to `struct { void *env; int val; }`) |
| Memory allocation | `malloc` + `realloc` per setjmp call site — known leak | no allocation; no leak |

**Migration path:** when lind-wasm upgrades to clang 19+, `wasm_eh_setjmp.c`
can be replaced with wasi-libc's `rt.c` directly. The new ABI eliminates the
per-thread table entirely, resolving the table memory leak. The `--translate-to-exnref`
binaryen pass can also be dropped at that point if clang 19+ emits exnref natively.

---

## Open tasks

### Table memory leak on normal function exit

`saveSetjmp` allocates a per-call-site table via `malloc` (initial 40 bytes,
grows via `realloc`). The EH catch block frees the table when a longjmp is
caught. However, if the protected scope exits *normally* (no longjmp occurs),
the table is never freed — a per-`setjmp`-call-site memory leak proportional
to the number of calls without a corresponding longjmp.

**Suggested fix:** emit a `free(table)` call on the normal exit path. This
requires either a compiler-side change to clang's wasm-sjlj lowering or a
wrapper in `wasm_eh_setjmp.c`.

### `saveSetjmp` table scan starts from index 0

`saveSetjmp` always searches for a free slot starting from index 0, making
each `setjmp` call O(n) in the number of active entries. A simple cached
`next_free` index stored alongside the table (reset on compaction or free)
would make the common case O(1).

---

## File map

| File | Role |
|------|------|
| `src/glibc/setjmp/wasm_eh_setjmp.c` | Runtime: `saveSetjmp`, `testSetjmp`, `__wasm_longjmp`, `getTempRet0`, `setTempRet0` |
| `src/glibc/setjmp/wasm_eh_c_longjmp_tag.c` | Weak `__c_longjmp` tag anchor for static builds without setjmp call sites |
| `src/glibc/setjmp/setjmp.h` | `sigsetjmp` macro; `sigprocmask` forward declaration |
| `src/glibc/sysdeps/i386/__longjmp.c` | `__longjmp` dispatch (`#ifdef LIND_EH_SETJMP`) |
| `src/glibc/sysdeps/i386/setjmp.c` | `__sigsetjmp` stub (mask save + return 0 in EH mode) |
| `src/glibc/sysdeps/x86/__longjmp_cancel.c` | `__longjmp_cancel` stub with correct 2-arg signature |
| `src/glibc/setjmp/longjmp.c` | `__libc_siglongjmp` → `__longjmp` → `__wasm_longjmp` |
| `src/lind-boot/src/lind_wasmtime/execute.rs` | Host-provided `__c_longjmp` tag for dynamic builds |
| `src/wasmtime/crates/wasmtime/src/runtime/linker.rs` | `new_child_linker`: per-child `__c_longjmp` tag for forked cages |
| `src/wasmtime/crates/lind-multi-process/src/signal.rs` | `signal_handler` returns `wasmtime::Result` to propagate `ThrownException` through epoch callback |
| `scripts/make_glibc_and_sysroot.sh` | Compiles EH setjmp objects; strips `-fPIE` for tag anchor |
| `scripts/make_shared_glibc.sh` | Excludes `wasm_eh_c_longjmp_tag.o` from `libc.so` link |
| `scripts/lind_compile` | Adds `-fwasm-exceptions -mllvm -wasm-enable-sjlj` when `LIND_ASYNCIFY_SETJMP` unset |
| `scripts/lind-wasm-opt` | Runs `--translate-to-exnref` when `LIND_ASYNCIFY_SETJMP` unset |
