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

For the **static build** there is no host linker, so `wasm_eh_c_longjmp_tag.o`
provides a weak local definition that `wasm-ld` can use to satisfy all imports
at link time (see [Known issues](#1-__c_longjmp-undefined-symbol-in-static-builds) below).

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

## Known issues and limitations

### 1. `__c_longjmp` undefined symbol in static builds

**Symptom:**
```
wasm-ld: error: libc.a(wasm_eh_setjmp.o): undefined symbol: __c_longjmp
```

**Root cause:** `__wasm_longjmp` uses `__builtin_wasm_throw(1, ...)` which
references the `__c_longjmp` wasm exception tag. The LLVM SjLj pass only emits
a weak tag definition in objects that contain a `_setjmp` call site. Programs
that never call `setjmp` themselves provide no such definition.

**Fix:** `src/glibc/setjmp/wasm_eh_c_longjmp_tag.c` — a synthetic object with
a dummy `_setjmp` call site (followed by a call to `__libc_write` to satisfy
the pass's requirement for a non-excluded post-setjmp call). The LLVM SjLj
pass emits a weak `__c_longjmp` tag definition into this object.

**Constraint:** This file must be compiled **without `-fPIE`**. With PIC/PIE
flags, the pass emits a tag *import* instead of a local weak definition.
`EXTRA_FLAGS_NO_PIE` in `scripts/make_glibc_and_sysroot.sh` strips `-fPIE`
for this specific compilation.

This file is stripped from `libc.so` before the shared link
(`scripts/make_shared_glibc.sh`) because the shared build uses host-provided
tag imports — a weak local definition in `libc.so` would be wrong.

### 2. `add-export-tool` failure with Tag section

**Symptom:**
```
Error: rewritten wasm is invalid
Caused by: unknown global 5: exported global index out of bounds
```

**Root cause:** `wasm_eh_c_longjmp_tag.o` introduces a wasm Tag section into
`libc.so`. The prebuilt `add-export-tool` binary was compiled with an older
`wasmparser` that does not account for the Tag section when counting globals,
miscounting subsequent global indices.

**Fix:** Strip `wasm_eh_c_longjmp_tag.o` from a temporary copy of `libc.a`
before the `wasm-ld` shared link:

```bash
SHARED_ARCHIVE=$(mktemp /tmp/libc_shared_XXXXXX.a)
cp "$SYSROOT_ARCHIVE" "$SHARED_ARCHIVE"
llvm-ar d "$SHARED_ARCHIVE" wasm_eh_c_longjmp_tag.o 2>/dev/null || true
trap "rm -f $SHARED_ARCHIVE" EXIT
```

### 3. GC heap SIGSEGV on first `throw`

**Symptom:** The first `longjmp` call caused SIGSEGV inside wasmtime's
JIT-compiled code, in the GC heap.

**Root cause:** Wasmtime allocates wasm exception objects on an internal GC
heap created via `Mmap::reserve()` (PROT_NONE), then grown via
`make_accessible()`. `Mmap::make_accessible` was a no-op in lind-wasm (rawposix
manages wasm linear memory permissions). The GC heap stayed PROT_NONE and the
first write to allocate an exception object faulted.

**Fix:** Restored `Mmap::make_accessible` to call real `mprotect`. This is
correct because the GC heap is a host-internal allocation (not wasm linear
memory) and must be writable. The PROT_NONE enforcement for wasm linear memory
is handled separately in `attach_shared_memory`.

### 4. `__longjmp_cancel` signature mismatch

**Symptom:**
```
wasm-ld: warning: function signature mismatch: __longjmp_cancel
>>> defined as (i32, i32) -> void in libc.a(longjmp.o)
>>> defined as () -> void in libc.a(__longjmp_cancel.o)
```

**Root cause:** The caller passes two arguments `(env, val)` but the stub had
`void __longjmp_cancel(void)`.

**Fix:** `src/glibc/sysdeps/x86/__longjmp_cancel.c` rewritten with the correct
2-argument signature. On wasm there is no shadow stack to unwind, so
`__longjmp_cancel` is identical to `__longjmp`.

### 5. `ThrownException` not propagating through the epoch callback

**Symptom:** `siglongjmp` called from a signal handler (delivered via epoch
interrupt) terminated the cage instead of returning to the `sigsetjmp` call site.

**Root cause:** Signal handlers are called via `signal_func.call()` in
`signal.rs`. When the signal handler called `siglongjmp` → `__wasm_longjmp`
throwing `__c_longjmp`, `call()` returned `Err(ThrownException)`. The old code
caught this, logged an error, and terminated the cage.

**Fix:** `signal_handler` now returns `wasmtime::Result<i32>`. When
`signal_func.call()` returns `Err(ThrownException)`:
1. Pop the signal asyncify frame.
2. Return `Err(err)` upward.
3. `epoch_callback` propagates the error via `?`.
4. Wasmtime receives the error from the epoch handler and re-throws the pending
   wasm exception in the original execution context.
5. The exception propagates to the `try_table` at the `sigsetjmp` call site.

Note: host functions registered with `func_wrap` that may surface wasm
exceptions must return `wasmtime::Result<T>`, not `anyhow::Result<T>`.

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

### Dead asyncify setjmp infrastructure in wasmtime store

The following code in `src/wasmtime/crates/wasmtime/src/runtime/store.rs`
is no longer used but was never removed:

- `stack_snapshots: HashMap<u64, Vec<u8>>` field
- `store_unwind_data()` / `retrieve_unwind_data()`
- `get_stack_snapshots()` / `set_stack_snapshots()`

No caller in `lind-boot/src/` invokes these methods. The `stack_snapshots`
field is copied on fork but is always empty. These should be removed.

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
| `src/wasmtime/crates/lind-multi-process/src/signal.rs` | `ThrownException` propagation through epoch callback |
| `scripts/make_glibc_and_sysroot.sh` | Compiles EH setjmp objects; strips `-fPIE` for tag anchor |
| `scripts/make_shared_glibc.sh` | Excludes `wasm_eh_c_longjmp_tag.o` from `libc.so` link |
| `scripts/lind_compile` | Adds `-fwasm-exceptions -mllvm -wasm-enable-sjlj` when `LIND_ASYNCIFY_SETJMP` unset |
| `scripts/lind-wasm-opt` | Runs `--translate-to-exnref` when `LIND_ASYNCIFY_SETJMP` unset |
