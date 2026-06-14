# marshal-infer

Automated inference of `lind_marshal_spec` descriptors for Lind library
interposition. Reads a **wasm32 LLVM bitcode** module (with DWARF) and infers,
per exported function, how each argument must be marshalled across cages —
the "Step 2" automation of the hand-written specs in
`tests/grate-tests/lib-interpose/lind_marshal.h`.

Design rationale, the reference-engine analyses (KSplit / CALI / idlc), and the
milestone plan live in:
- `plan-inference.md` (repo root) — the plan
- `impl-log-inference.md` (repo root) — running implementation log
- `research/arg-marshalling/{ksplit,cali,idlc}-source-analysis.md` — source studies

## Build

```bash
tools/marshal-infer/build.sh          # configure + build
tools/marshal-infer/build.sh clean    # from scratch
```

Builds against the repo's `clang+llvm-18.1.8` tree (static libs). Output binary:
`tools/marshal-infer/build/marshal-infer`.

## Use

```bash
# 1. emit wasm32 bitcode + DWARF for the target library
lind-clang --emit-llvm path/to/lib.c          # -> path/to/lib.bc

# 2. run inference
tools/marshal-infer/build/marshal-infer path/to/lib.bc
```

`--all` also analyzes internal/static functions (default: interface candidates
only, i.e. non-local-linkage definitions).

`--exports <file>` restricts output to functions named in a newline-separated
list (e.g. a library's exported symbols); multiple `.bc` inputs are aggregated
and deduped by name.

## Output (JSON inference record)

Per exported function: `ret` (void/scalar/ptr_alias_arg/force_local/handle/
ptr_unknown), and per arg a node with `kind` (SCALAR/PTR/STRUCT/UNION),
`dir` (in/out/inout), `size_kind` (const/from_arg/from_arg_pointee/cstr/unknown),
`size_arg`/`const_size`, nested `pointee`/`fields` (with wasm32 `offset` +
`touched`), `handle`/`handle_class`, and a `warnings` residue list. The format is
intentionally a superset/rough cover of `lind_marshal.h`'s `lind_marshal_spec`.

## Generating sidecars in the build

- **Any library:** `lind-clang --emit-marshal --compile-library lib.c` drops
  `lib.marshal.json` next to the artifact.
- **libc (glibc):** `tools/marshal-infer/infer_libc.sh` → `libc.marshal.json`
  scoped to the exports of the built `libc.cwasm` (~87% of 2118 exports; the rest
  are sysdeps TUs needing per-dir flags — see `impl-log-inference.md`).
- **One glibc TU:** `tools/marshal-infer/glibc_emit_bc.sh string/strncpy.c`.

## Status

- **M0–M5 (done):** DWARF parameter trees; direction / size / pointer-class /
  handle / return inference over IR; nested-struct projection; JSON sidecar;
  alias resolution; build integration (`lind_compile --emit-marshal`) and the
  libc runner. All 8 `auto-*` oracle shapes reproduced; `run_tests.sh` green.
- **Next (optional):** push libc coverage toward 100% by emitting bitcode inside
  the glibc build (CC wrapper beside each `.o`); FROM_ARG_POINTEE for OUT buffers
  (compress2 `dest`); precise `touched` if caller-side IR becomes available.
