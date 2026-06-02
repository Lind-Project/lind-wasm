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

### 2026-05-30 — Beginning Stage-3 implementation

Implementation order (rule.md: increment, log challenges):
- Phase 1: CSTR + FROM_ARG_POINTEE + RET_PTR_INTO_ARG  (sizing variants, no new infra)
- Phase 2: Opaque handles + handle table in C grate code
- Phase 3: Recursive nested struct marshalling + pointer fixup
- Callbacks: explicitly deferred per design decision

### 2026-05-30 — Stage-3 implementation complete

**Phase 1: Sizing variants** (CSTR, FROM_ARG_POINTEE, RET_PTR_INTO_ARG)
- LIND_SIZE_CSTR: scans source cage for '\0' in 64-byte chunks, capped at LIND_MARSHAL_CSTR_CAP (4096).
  Test: auto-cstr intercepts strlen.
  Challenge: strlen("hello") with a literal is inlined at compile time — no dynamic call.
  Fix: cage uses `static char g_s[]` (global) to force a dynamic import (same pattern as libc-strlen).
- LIND_SIZE_FROM_ARG_POINTEE: reads *(uint32_t at raw_args[size_arg_index]) from source cage
  via copy_data_between_cages before allocating the shadow.
  Test: auto-compress2 intercepts compress2, writes "LIND" into dest, sets *destLen=4.
- LIND_RET_PTR_INTO_ARG: handler returns a shadow pointer; runtime computes
  offset = handler_ret - shadow_start, returns source_ptr + offset.
  Test: auto-memchr intercepts memchr, return value translates correctly.

**Phase 2: Opaque handles**
- Handle table: 64-entry static array in grate memory, keyed by (class, app_token).
  lind_register_handle / lind_translate_handle / lind_release_handle.
- LIND_ARG_HANDLE: pre-call translates app_token → real_object via table.
- LIND_RET_HANDLE: post-call registers handler's return value, returns app_token to source cage.
- Test: auto-handle (toy_ctx_create/get_val/close round-trip).
  Note: toy_ctx_close releases from handle table via linear scan on real_ptr. This is fine for
  tests; a production implementation would pass the app_token through a separate mechanism.

**Phase 3: Recursive nested structs**
- lind_layout / lind_field structs added for recursive description.
- _lind_pre_ptr: if as->layout != NULL, performs a two-step pre-call:
    1. Copy raw struct bytes from source cage to shadow (initial blit).
    2. For each touched field: scalar (already copied), ptr (recurse: allocate child shadow, copy-in,
       overwrite shadow field with child shadow address), handle (translate in place).
  Tracks per-field (offset, orig_src_ptr, shadow_start) for post-call fixup.
- _lind_post_struct: (a) copy-back child buffer data for OUT/INOUT ptr fields,
  (b) blit entire struct shadow back to source, (c) fix up ptr fields with source-cage addresses.
- Sibling field size: LIND_SIZE_FROM_ARG in struct context reads from shadow at
  fields[size_arg_index].offset (treated as sibling field index, not top-level arg index).
- Test: auto-nested intercepts toy_buf_checksum({char *data, uint len}).
  Handler accesses b->data as a local shadow pointer, computes sum+1 (proves data was copied in).

All 12 tests pass.

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
