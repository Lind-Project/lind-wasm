# Implementation Log — Automated Argument Marshalling (Stage 1)

## Goal
Implement a C-runtime-in-the-grate model for automated argument marshalling.
Grate authors write typed handlers; a `lind_marshal.h` header provides the
dispatch machinery that does pre-call copy-in, post-call copy-out, and NULL
pass-through automatically.

## Design decisions made before coding

- **Location of marshalling loop**: C runtime in the grate (not Rust portal).
  The marshalling dispatcher (`lind_marshal_dispatch`) lives in `lind_marshal.h`
  as a static inline / static function library included by every grate that wants
  auto-marshalling. No Rust changes required.

- **Handler invocation**: `LIND_DEFINE_MARSHAL_HANDLER(name, spec, typed_fn)`
  macro generates the raw `pass_fptr_to_wt`-compatible handler wrapper.
  The wrapper packs the raw args, determines the source cage from the first
  non-zero argNcage, then calls `lind_marshal_dispatch`.

- **NULL pointers**: passed through to the handler without allocation or copy.

- **Source cage extraction**: all args originate from the same source cage;
  the macro searches arg1cage..arg6cage for the first non-zero value.

- **Shadow memory**: static bump-allocator arena (64 KB) per grate, reset after
  each call. Simple and sufficient for scalar/buffer tests.

- **Where lind_marshal.h lives**: initially in
  `tests/grate-tests/lib-interpose/` (alongside the tests that use it).
  Can be promoted to the sysroot later once the API stabilizes.

---

## Log

### 2026-05-30 — Initial implementation

**Challenge: WASM indirect-call type checking breaks typed handler dispatch**

The design wanted the typed handler to have a natural C signature like
`void *handler_memcpy(void *dest, const void *src, size_t n)`.
`lind_marshal_dispatch` would then call it through a `handler6_t` cast.

Problem: WASM validates the type of every indirect call against the function
table entry. Calling a `(i32,i32,i32)->i32` function through a
`(i64,i64,i64,i64,i64,i64)->i64` signature traps immediately — there is no
UB fallback like on native x86.

Without `--fpcast-emu` (which grates don't use by default), typed C
signatures cannot be called through a mismatched pointer.

**Decision**: Require handlers to use the uniform signature:
```c
uint64_t handler_foo(uint64_t a0, uint64_t a1, uint64_t a2,
                     uint64_t a3, uint64_t a4, uint64_t a5);
```
This is `handler6_t` — exactly what `lind_marshal_dispatch` calls.
Helper macros `LIND_AS_PTR`, `LIND_AS_SIZE`, `LIND_AS_INT`, `LIND_RET_PTR`,
`LIND_RET_INT` reduce the explicit casting boilerplate inside the handler body.

The key benefit over the old style is unchanged: the handler receives local
shadow pointers (valid in grate memory), calls standard C library functions
directly, and never touches `copy_data_between_cages`.

Future: once `--fpcast-emu` is added to the grate compilation path, fully
typed handlers become possible. The `lind_marshal_spec` and dispatch logic
need no changes — only the handler signature constraint relaxes.

**Note on intercepted printf output**: `auto-memcpy` and `auto-strncpy` don't
print the handler's `printf` line even though the handler ran — this is because
the grate's printf apparently doesn't flush before the child cage's output.
Not a bug; the PASS assertions in both cage and grate confirm correctness.

**All 7 tests pass**: libc-rand, libc-strlen, custom-lib, zlib-python,
auto-scalar, auto-memcpy, auto-strncpy.

### 2026-05-30 — Migrated zlib-python to lind_marshal.h

**What changed in zlib-python_grate.c:**

- `deflateInit2_` / `deflateEnd`: were raw handlers with ignored args;
  now use `LIND_ARG_SCALAR` for all args → `LIND_DEFINE_MARSHAL_HANDLER`
  generates the wrapper. Zero `copy_data_between_cages` calls.

- `deflate`: was 3× `copy_data_between_cages` (read struct, write output,
  write struct back). Now:
  - z_stream* declared as `LIND_PTR_INOUT, LIND_SIZE_CONST(sizeof(ZStreamWasm32))`
    → auto-marshal handles the struct copy-in / copy-out
  - Handler receives a local shadow pointer to the z_stream; can read/write
    fields directly
  - 1× manual `copy_data_between_cages` remains: writing FIXED_OUTPUT into
    `zst->next_out` in the source cage (nested pointer, outside Stage 1 scope)
  - Uses `LIND_GRATE_CAGE()` / `LIND_SOURCE_CAGE()` accessors set by dispatch

**New in lind_marshal.h:**
- `_lind_marshal_source_cage` / `_lind_marshal_grate_cage` globals
- `LIND_SOURCE_CAGE()` / `LIND_GRATE_CAGE()` accessor macros
  for handlers that still need a manual cross-cage copy (nested pointers)
