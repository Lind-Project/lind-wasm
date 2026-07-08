# 02-strings — design: marshalling a string argument

This iteration adds the first function that crosses the guest memory boundary:
`size_t str_len(const char *)`, whose real implementation runs inside the
lind/wasmtime sandbox. It builds on `01-scalars` (which proved the runtime +
`.so` packaging plumbing) by adding exactly one new capability: **copy-in
marshalling of a pointer argument**.

## Why a pointer can't just be passed through

The guest runs in its own wasm linear memory. A native `const char*` is a host
address that is meaningless inside the guest — the guest would read whatever sits
at that offset in *its* memory. So the host stub must copy the string **into**
guest memory and pass the resulting **guest offset**. The return value here is a
plain `size_t` (a scalar), so nothing is marshalled on the way out.

Two supporting mechanisms make this possible:

- **A guest allocator.** The host can *write* anywhere in the guest's linear
  memory, but it must not *decide where* — only the guest's allocator knows which
  regions are free. So the guest exports `guest_malloc`/`guest_free`, and the host
  copies into a block the guest hands it.
- **libc, preloaded.** The guest calls real `malloc`/`strlen`, so its module
  imports `env::malloc`, `env::strlen`. Those are resolved by **preloading**
  `libc.cwasm`/`libm.cwasm` into the sandbox. The guest's constructors run at
  instantiation (`_initialize`), so libc is initialized before the first
  `guest_malloc`.

## End-to-end flow of `str_len("hello")`

```
app: str_len("hello")
  │  native call → resolves to the stub symbol in libstrdemo.so
  ▼
STUB (generated, extern "C")
  1. cstr_bytes("hello")  → Vec<u8> = [h,e,l,l,o,\0]   (host string incl. NUL)
  2. call_buf("str_len", &[Arg::Buf(&b0)])
  ▼
SandboxedLib::call  (lock the global Mutex, then marshal-in)
  3. copy_in(bytes):
       a. guest_malloc(6)  → guest calls libc malloc INSIDE the sandbox → guest offset `p`
       b. guest_mem()      → host base pointer + size of the guest's (shared) linear memory
       c. bounds-check [p, p+6) ≤ size
       d. copy_nonoverlapping host bytes → base + p   (string now lives in guest memory)
  4. params = [Val::I32(p)]
  5. func.call(store, params, results)  ── enter the sandbox ──▶ guest str_len(p) → 5
  6. guest_free(p)          (freed even if the call errored)
  7. results[0] = I32(5) → widen to i64
  ▼
STUB: 5 as usize → returned to the app
```

## Changes by file

The work splits into a **reusable marshalling engine** (in `lind-boot`, survives
beyond this example), **code generation**, **build wiring**, and the **example**.

### A. Marshalling engine — `lind-boot`

**`src/lind-boot/src/lind_wasmtime/sandboxed_lib.rs`** — the core. Added:

- **`Arg` enum** — a host-side argument: `I32(i32)` (scalar, passes straight
  through) or `Buf(&[u8])` (copied into the guest, passed as its offset).
- **`SandboxedLib::call(name, &[Arg]) -> Result<i64>`** — copies each `Arg::Buf`
  into the guest (recording offsets to free), builds the `Val` params, invokes the
  export, **frees the buffers even on error**, and returns the first result widened
  to `i64`.
- **`guest_malloc` / `guest_free`** — `get_typed_func::<i32,i32>` / `::<i32,()>`
  on the guest's exports, called to allocate/free inside the sandbox.
- **`guest_mem() -> (*mut u8, usize)`** — base pointer + size of the guest's linear
  memory. lind's dynamic build uses a **shared, imported** memory, so it is *not* a
  named `"memory"` export and *not* an `unshared()` `wasmtime::Memory`. Instead we
  take the first memory via `store.as_context_mut().0.all_memories().next()` and read
  `shared_base_ptr()` + the length off the `VMMemoryDefinition` — mirroring the
  syscall layer's `get_memory_base_and_size`.
- **`copy_in(bytes) -> u32`** — `guest_malloc` the length, fetch base+size *after*
  (so a growth is reflected), bounds-check `[off, off+len) ≤ size`, then
  `copy_nonoverlapping` the bytes into `base+off`.
- **Imports** — added `AsContextMut` (for `as_context_mut()`).
- `call_scalar` (from 01) is untouched.

**`src/lind-boot/src/lind_wasmtime/mod.rs`**, **`src/lind-boot/src/lib.rs`** —
re-export `Arg` alongside `SandboxedLib` / `init_sandboxed_lib`, so the generated
stub can name `lind_boot::Arg`.

**`src/lind-boot/src/cli.rs`** — libc preloading into the embedding path.
`for_sandboxed_lib` now sets `preloads: preloads_from_env()`. The new
`preloads_from_env()` reads the `LIND_PRELOAD` env var (comma-separated
`name=path`, same syntax as the CLI `--preload`, parsed by the existing
`parse_preloads`). Because the `.so` isn't chrooted into lindfs, callers pass
**host** paths. The rest of the preload machinery (`prepare_main_instance` loading
the modules, dylink/GOT resolution, running `_initialize`) already existed and is
shared with the binary.

### B. Code generation — `lind-sharedlib-poc/tools/gen_stubs.sh`

Taught the generator about pointer arguments:

- **Classifier pass** — scans `functions.txt` for whether any function needs
  marshalling (a non-`i32` arg/return) vs. is all-scalar.
- **Conditional header** — emits `use core::ffi::{CStr, c_char};`, pulls in
  `lind_boot::Arg`, and the `cstr_bytes()` + `call_buf()` helpers **only when** a
  marshalled function is present; the scalar `call(&[i32])` helper only when an
  all-scalar function is present (so 01's output shape is preserved).
- **Per-function emission** — all-`i32` → trivial pass-through stub (unchanged); a
  `cstr` arg or non-`i32` return → a marshalled stub that reads each `cstr` into a
  `Vec<u8>` incl. NUL, wraps it in `Arg::Buf`, calls `call_buf`, and casts the
  `i64` result to the declared return. Arg types understood: `i32`, `cstr`; return
  types: `i32`, `usize`, `i64`.

### C. Build wiring

**`lind-sharedlib-poc/common.mk`** — added a `PRELOAD ?=` variable, exported as
`LIND_PRELOAD="$(PRELOAD)"` in the `run` recipe (empty ⇒ no preloads).

**`lind-sharedlib-poc/README.md`** — example index: `02-strings` added, buffers
renumbered to `03`.

**`lind-sharedlib-poc/examples/01-scalars/stub/src/lib.rs`** — regenerated by the
evolved generator (only the banner shifted; the `add`/`subtract` stubs are
unchanged).

### D. The example — `lind-sharedlib-poc/examples/02-strings/`

- **`guest.c`** — exports `str_len` (wraps libc `strlen`) plus
  `guest_malloc`/`guest_free` (wrap libc `malloc`/`free`), via `export_name`; no
  `main`. Named `str_len`, **not** `strlen`, so the `.so` doesn't shadow libc's
  `strlen` for the host process.
- **`demo.c`** — unmodified native caller; prints libc `strlen` alongside as a
  cross-check.
- **`functions.txt`** — one line: `str_len  usize  cstr`.
- **`Makefile`** — `LIB := strdemo`, `include ../../common.mk`, and
  `PRELOAD := env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm`.
- **`stub/Cargo.toml`** — `[lib] name = "strdemo"` → `libstrdemo.so`; `lind-boot`
  path dep.
- **`stub/src/lib.rs`** *(generated)* — the `str_len(*const c_char) -> usize` stub
  + `cstr_bytes`/`call_buf` helpers.

## Run

```bash
make run          # build + run against the wasm-sandboxed libstrdemo.so
make run-native   # baseline: real native libstrdemo.so, no sandbox
```

Expected (identical for both):

```
str_len("") = 0 (native strlen = 0)
str_len("hello") = 5 (native strlen = 5)
str_len("lind sandbox") = 12 (native strlen = 12)
```

## Still out of scope

Out-params / filled buffers (host allocates, guest writes back), structs,
callbacks, and concurrency. This example proves **copy-in of one buffer** only.
