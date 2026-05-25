# Wasm EH-Based setjmp/longjmp Implementation

This document covers the design, implementation, and challenges of the
Wasm exception-handling (EH) based setjmp/longjmp on the `setjmp-alt-impl`
branch.

## Background: Two Modes

Lind-wasm supports two setjmp/longjmp implementations, selected at glibc
build time:

| Mode | Flag | Mechanism |
|------|------|-----------|
| **EH** (default) | `-DLIND_EH_SETJMP` | Wasm exception tags, `__builtin_wasm_throw` |
| **Asyncify** (opt-in) | `LIND_ASYNCIFY_SETJMP=1` | `lind.lind-longjmp` host import |

`siglongjmp` always uses the asyncify import regardless of mode — see
[Why siglongjmp stays asyncify](#why-siglongjmp-stays-asyncify) below.

---

## Core Mechanism: the LLVM SjLj Pass

When user code is compiled with `-fwasm-exceptions -mllvm -wasm-enable-sjlj`,
the LLVM `WebAssemblyLowerEmscriptenEHSjLj` pass rewrites every `_setjmp`
call site in the user's object:

```c
// User writes:
val = setjmp(buf);

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

And `longjmp(buf, val)` is lowered to `__wasm_longjmp(buf, val)`, which
throws the `__c_longjmp` wasm exception.

The runtime support functions are implemented in
`src/glibc/setjmp/wasm_eh_setjmp.c`:

- **`saveSetjmp(buf, label_id, table, size)`** — stores `(buf, label_id)` in
  a per-thread table; grows the table via `realloc` when full; writes the
  new size via `setTempRet0`.
- **`testSetjmp(env, table, size)`** — searches the table for a matching
  `buf` address; returns the `label_id` of the matching frame, or 0 if the
  exception belongs to an outer frame (must rethrow).
- **`getTempRet0` / `setTempRet0`** — thread-local "second return value"
  used because wasm imported functions cannot return multiple values yet.
- **`__wasm_longjmp(buf, val)`** — stores the normalised return value in
  `buf[3]`, a self-pointer in `buf[2]`, then calls
  `__builtin_wasm_throw(1, &buf[2])` to throw `__c_longjmp`.

### `jmp_buf` layout

`jmp_buf` is `long int[8]` (32 bytes on wasm32). The EH implementation
uses four 4-byte slots:

| Slot | Set by | Purpose |
|------|--------|---------|
| `buf[0]` | `saveSetjmp` | self-pointer for frame matching in `testSetjmp` |
| `buf[1]` | — | unused / reserved |
| `buf[2]` | `__wasm_longjmp` | self-pointer passed as exception payload |
| `buf[3]` | `__wasm_longjmp` | normalised return value (`val ?: 1`) |

### `__longjmp` dispatch

`src/glibc/sysdeps/i386/__longjmp.c` selects the implementation at compile
time:

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

## `sigsetjmp` Design

### Why `sigsetjmp` must expand to `_setjmp` at the call site

The LLVM SjLj pass only instruments `_setjmp` call sites that appear
directly in user compilation units. If `sigsetjmp` remained a function
call to `__sigsetjmp`, the pass would see a regular function call — not
a `_setjmp` — and emit no `try_table` at the call site. A subsequent
`longjmp` would throw `__c_longjmp` with nowhere to land.

The fix is to make `sigsetjmp` a **macro** that expands to `_setjmp`
in user code, where the pass can see and transform it:

```c
// src/glibc/setjmp/setjmp.h
#define sigsetjmp(env, savemask)                                        \
  (__extension__ ({                                                      \
    int __sm = (savemask);                                              \
    (env)[0].__mask_was_saved =                                         \
      __sm && (sigprocmask(SIG_BLOCK, (__sigset_t *)NULL,               \
                           (__sigset_t *)&(env)[0].__saved_mask) == 0); \
    _setjmp(env);   /* ← LLVM transforms this */                        \
  }))
```

The signal mask save happens inline before `_setjmp`; restoration on
`siglongjmp` is handled by `__libc_siglongjmp` as before.

`__sigsetjmp` in `sysdeps/i386/setjmp.c` still exists for any internal
glibc use, but is now just a mask-save + `return 0` — the setjmp
machinery is always at the call site.

### `sigprocmask` forward declaration

The macro calls `sigprocmask` before `<signal.h>` may have been included.
A forward declaration was added inside `<setjmp.h>`, guarded by
`#ifndef _SIGNAL_H`:

```c
#ifndef _SIGNAL_H
extern int sigprocmask(int __how, const __sigset_t *__restrict __set,
                       __sigset_t *__restrict __oset) __THROW;
#endif
```

The guard prevents a type conflict when glibc itself includes `<signal.h>`
first: glibc's `signal.h` uses `sigset_t`, while this declaration uses
`__sigset_t`, and clang treats them as distinct types even though they
are the same underlying type.

`sigprocmask` (the public POSIX name) is used rather than the private
`__sigprocmask` alias so that dynamic builds resolve it from `libc.so`'s
exported symbol table.

### Why `siglongjmp` stays asyncify

`siglongjmp` → `__libc_siglongjmp` → `__longjmp` → `__wasm_longjmp`
would throw `__c_longjmp`. In principle this reaches the `try_table` at
the `sigsetjmp` call site — but only if the throw propagates there
uninterrupted.

The problem: signal handlers in lind-wasm are invoked through a Rust host
boundary (`signal_func.call()` in `signal.rs`). A wasm EH exception
**cannot propagate through a Rust stack frame**; `call()` catches it and
returns `Err(ThrownException)`. With asyncify, the signal unwind/rewind
path was already designed to cross this boundary cleanly.

Therefore `siglongjmp` continues to import and call `lind.lind-longjmp`
(asyncify), while `sigsetjmp` uses the EH path for the setjmp side.

---

## Challenges and Fixes

### 1. GC heap SIGSEGV on first `throw`

**Symptom:** The first call to `__builtin_wasm_throw` (i.e. the first
`longjmp`) caused a SIGSEGV inside wasmtime's JIT-compiled code, in the
GC heap.

**Root cause:** Wasmtime uses a GC heap internally to allocate wasm
exception objects. The heap is created via `Mmap::reserve()` (PROT_NONE)
then grown on demand via `make_accessible()`. In lind-wasm,
`Mmap::make_accessible` is a **no-op** — rawposix manages wasm linear
memory permissions, so wasmtime's mprotect calls are skipped. This is
correct for wasm linear memory (rawposix sets up permissions via
MAP_FIXED anyway), but fatal for the GC heap: it stayed PROT_NONE, and
the first write to allocate an exception object caused SIGSEGV.

**Fix** (`src/wasmtime/crates/wasmtime/src/runtime/vm/memory/mmap.rs`,
`MmapMemory::new`):

```rust
// Before:
let mmap = Mmap::accessible_reserved(HostAlignedByteCount::ZERO, request_bytes)?;

// After:
// lind-wasm: make_accessible is a no-op (rawposix manages wasm memory
// permissions), so pre-allocate the entire region as accessible. Guards
// being host-accessible is safe because lind-wasm uses explicit bounds
// checks, not SIGSEGV-on-PROT_NONE, for out-of-bounds detection.
// This also fixes GC heap allocation, which is host-internal and needs
// to be writable.
let mmap = Mmap::accessible_reserved(request_bytes, request_bytes)?;
```

`accessible_reserved(size, size)` calls `Mmap::new(size)` directly
(PROT_READ|PROT_WRITE), bypassing the `reserve()` + `make_accessible()`
path. Since lind-wasm relies on explicit bounds checks (not memory
protection faults) for out-of-bounds detection, the guards being
host-accessible is safe.

---

### 2. `__c_longjmp` undefined symbol in static builds

**Symptom:**
```
wasm-ld: error: libc.a(wasm_eh_setjmp.o): undefined symbol: __c_longjmp
```

**Root cause:** `__wasm_longjmp` uses `__builtin_wasm_throw(1, ...)`,
which references the `__c_longjmp` wasm exception tag. The LLVM SjLj
pass only emits a weak tag definition in objects that contain a `_setjmp`
call site. Programs that never call `setjmp` themselves provide no such
definition, leaving the symbol undefined.

**Fix:** Added `src/glibc/setjmp/wasm_eh_c_longjmp_tag.c` — a synthetic
object with a dummy `_setjmp` call site (followed by a call to
`__libc_write` to satisfy the pass's requirement for a non-excluded
post-setjmp call). The LLVM SjLj pass emits a weak `__c_longjmp` tag
definition into this object, which `wasm-ld` can use as a fallback for
programs that have no setjmp call sites of their own.

**Complication:** This file must be compiled **without `-fPIE`**. With
PIC/PIE mode, the pass emits a tag *import* instead of a local weak
definition. `EXTRA_FLAGS` in `scripts/make_glibc_and_sysroot.sh`
contained `-fPIE`, so it is stripped for this specific compilation:

```bash
EXTRA_FLAGS_NO_PIE="${EXTRA_FLAGS//-fPIE/}"
clang $CFLAGS_NO_PIC $WARNINGS $EXTRA_FLAGS_NO_PIE \
    ... -fwasm-exceptions -mllvm -wasm-enable-sjlj \
    -o $BUILD/setjmp/wasm_eh_c_longjmp_tag.o \
    -c $GLIBC/setjmp/wasm_eh_c_longjmp_tag.c
```

---

### 3. `add-export-tool` failure in shared build

**Symptom:**
```
Error: rewritten wasm is invalid
Caused by: unknown global 5: exported global index out of bounds (at offset 0x1167)
```

**Root cause:** `wasm_eh_c_longjmp_tag.o` introduces a wasm **Tag
section** into `libc.so`. The prebuilt `add-export-tool` binary was
compiled with an older `wasmparser` that does not account for the Tag
section when counting globals. It miscounts global indices, and reports
global[5] (`__tls_base`, the first locally-defined global) as out of
bounds even though it is valid.

**Fix** (`scripts/make_shared_glibc.sh`): Strip `wasm_eh_c_longjmp_tag.o`
from a temporary copy of `libc.a` before the `wasm-ld` shared link:

```bash
SHARED_ARCHIVE=$(mktemp /tmp/libc_shared_XXXXXX.a)
cp "$SYSROOT_ARCHIVE" "$SHARED_ARCHIVE"
llvm-ar d "$SHARED_ARCHIVE" wasm_eh_c_longjmp_tag.o 2>/dev/null || true
trap "rm -f $SHARED_ARCHIVE" EXIT
# use "$SHARED_ARCHIVE" instead of "$SYSROOT_ARCHIVE" in wasm-ld
```

This is safe: the tag anchor is only needed in the static `libc.a` as a
fallback. In the shared build, every user compilation unit that uses
`setjmp` generates its own `__c_longjmp` definition via the LLVM SjLj
pass.

---

### 4. `__longjmp_cancel` signature mismatch

**Symptom:**
```
wasm-ld: warning: function signature mismatch: __longjmp_cancel
>>> defined as (i32, i32) -> void in libc.a(longjmp.o)
>>> defined as () -> void in libc.a(__longjmp_cancel.o)
```

**Root cause:** `sysdeps/unix/sysv/linux/x86/longjmp.c` (the actual
source for `longjmp.o`) calls
`__longjmp_cancel(env[0].__jmpbuf, val ?: 1)` with two arguments. The
stub at `sysdeps/x86/__longjmp_cancel.c` had `void __longjmp_cancel(void)`
— no arguments.

**Fix:** Rewrote the stub with the correct 2-arg signature:

```c
#include <setjmp.h>
extern void __longjmp(__jmp_buf env, int val) __attribute__((__noreturn__));

void __longjmp_cancel(__jmp_buf env, int val) {
    __longjmp(env, val);
}
```

On wasm there is no shadow stack to unwind, so `__longjmp_cancel` is
identical to `__longjmp`.

---

### 5. `ThrownException` not propagating through the epoch callback

**Symptom:** `test_sigsetjmp` (siglongjmp called from an alarm handler
delivered via epoch interrupt) caused the cage to terminate instead of
returning to the `sigsetjmp` call site.

**Root cause:** Signal handlers are called via `signal_func.call()` in
`signal.rs`. When the signal handler called `siglongjmp` → `__wasm_longjmp`
throwing `__c_longjmp`, `call()` returned `Err(ThrownException)`. The
old code caught this, printed an error, and terminated the cage.

**Fix:** Changed `signal_handler`'s return type from `i32` to
`wasmtime::Result<i32>`. When `signal_func.call()` returns
`Err(ThrownException)`:

1. Pop the signal asyncify frame (the asyncify rewind path will not run).
2. Return `Err(err)` instead of terminating the cage.
3. `epoch_callback` propagates the error via `?`.
4. Wasmtime receives the error from the epoch handler and re-throws the
   pending wasm exception in the original wasm execution context (the
   epoch check loop).
5. The exception propagates up to the `try_table` at the `sigsetjmp`
   call site and is caught normally.

One subtle point: host functions registered with `func_wrap` that may
surface wasm exceptions must return `wasmtime::Result<T>`, **not**
`anyhow::Result<T>`. `WasmRet` is only implemented for `wasmtime::Result`.

---

---

## Cross-Module Tag Sharing in Dynamic Builds

### The problem

In lind-wasm's dynamic build, `libc.so` and user code are separate wasmtime
`Instance` objects. Wasm exception tags are module-instance-level constructs:
each instance gets its own runtime tag value, even if both name it
`__c_longjmp`. A throw using instance A's tag is not caught by instance B's
catch, even if both call it the same name.

The asyncify `lind.lind-longjmp` import sidesteps this entirely (it operates
through memory state, not wasm exceptions). The EH path must solve it
explicitly.

### The solution: host-provided shared tag

The wasmtime host creates **one** `Tag` object per cage `Store` and registers
it in the `Linker` under `"env"."__c_longjmp"`:

```rust
// src/lind-boot/src/lind_wasmtime/execute.rs
let tag_type = TagType::new(FuncType::new(&engine, [ValType::I32], []));
let tag = Tag::new(&mut *wstore, &tag_type)?;
linker_guard.define(&*wstore, "env", "__c_longjmp", tag)?;
```

The same is done for forked children in `linker.rs:new_child_linker`.

When all module instances — user code, `libc.so`, any dynamically loaded
library — import `"env"."__c_longjmp"`, they all receive the same `Tag`
object. A throw in `libc.so` (from `__wasm_longjmp`) and a catch in user
code (try_table inserted by the SjLj pass) reference identical tag identity.
The exception crosses the module boundary and is caught correctly.

### Making all modules use imports

All glibc objects are compiled with `-fPIC` (for shared-library
compatibility). In PIC mode, the LLVM SjLj pass emits a tag **import** for
`__c_longjmp` rather than a local weak definition. This is correct: the
host owns the authoritative definition.

To extend this to glibc's *internal* `__sigsetjmp` call sites (notably
`elf/dl-catch.c`'s `_dl_catch_exception` and pthread cancellation), glibc
is now compiled globally with `-fwasm-exceptions -mllvm -wasm-enable-sjlj`
in `GLIBC_SETJMP_CFLAGS`. This causes the SjLj pass to instrument all
`_setjmp`/`__sigsetjmp` call sites inside glibc itself, making their
catches compatible with the shared host tag.

For the **static build** there is no host linker, so `wasm_eh_c_longjmp_tag.o`
(compiled without `-fPIC`) provides a weak local definition that `wasm-ld`
can use to satisfy all the imports at link time. The host also defines the
tag in `execute.rs`, but the static binary's local definition takes
precedence (the host definition satisfies no imports in a self-contained
binary).

### Affected glibc paths now fixed

| Path | Function | What it does |
|------|----------|--------------|
| Dynamic linker error handling | `_dl_catch_exception` in `elf/dl-catch.c` | Catches errors thrown by `dlopen`/`dlsym`/`dlclose` internals via `__sigsetjmp`/longjmp |
| pthread cancellation | `pthread_cleanup_push` macro (via `__sigsetjmp_cancel`) | Runs cleanup handlers when a thread is cancelled via longjmp |

Without the SjLj pass applied to these glibc files, any internal longjmp on
an error path would throw `__c_longjmp` with no matching catch in place,
propagating past the intended handler and crashing or producing undefined
behaviour.

---

## File Map

| File | Role |
|------|------|
| `src/glibc/setjmp/wasm_eh_setjmp.c` | Runtime: `saveSetjmp`, `testSetjmp`, `__wasm_longjmp`, `getTempRet0`, `setTempRet0` |
| `src/glibc/setjmp/wasm_eh_c_longjmp_tag.c` | Weak `__c_longjmp` tag anchor for programs without setjmp |
| `src/glibc/setjmp/setjmp.h` | `sigsetjmp` macro; `sigprocmask` forward decl |
| `src/glibc/sysdeps/i386/__longjmp.c` | `__longjmp` dispatch (`#ifdef LIND_EH_SETJMP`) |
| `src/glibc/sysdeps/i386/setjmp.c` | `__sigsetjmp` (mask save + return 0 in EH mode) |
| `src/glibc/sysdeps/x86/__longjmp_cancel.c` | `__longjmp_cancel` stub with correct 2-arg signature |
| `src/wasmtime/crates/wasmtime/src/runtime/vm/memory/mmap.rs` | GC heap fix: pre-allocate entire region |
| `src/wasmtime/crates/lind-multi-process/src/signal.rs` | `ThrownException` propagation through epoch callback |
| `scripts/make_glibc_and_sysroot.sh` | Compile `wasm_eh_setjmp.o` and `wasm_eh_c_longjmp_tag.o` (no `-fPIE` for the latter) |
| `scripts/make_shared_glibc.sh` | Exclude `wasm_eh_c_longjmp_tag.o` from `libc.so` link |
