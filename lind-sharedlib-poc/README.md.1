# Sandboxed shared libraries — PoC

Take a library, compile it to WebAssembly, run it as a guest inside the
lind/wasmtime sandbox, and expose it to the outside world as an ordinary native
`.so`. An **unmodified** native application links the `.so` and calls its
functions normally — unaware that the real work happens inside a wasm sandbox.
A **binary drop-in** for an unmodified app, not a recompile against a wrapper API.

## How this folder is organized

The reusable runtime lives in `src/` (the `lind-boot` refactor: `--call`,
`init_sandboxed_lib`, `SandboxedLib`). **This** tree holds only the per-iteration
material — one self-contained folder per example, each adding exactly one new
marshalling capability on top of the previous:

| Example | Adds |
| --- | --- |
| [`examples/01-scalars`](examples/01-scalars) | plumbing only — scalar `int(int,int)`, no marshalling |
| `examples/02-buffers` *(next)* | caller-allocated in/out buffer |
| `examples/03-strings` | NUL-terminated string copy across the boundary |
| `examples/04-structs` | struct copy + ILP32/LP64 layout |
| `examples/05-callbacks` | guest trampoline re-entering the host |
| `examples/06-concurrency` | drop the global lock |

### Anatomy of an example

Every example has the same shape, so the next one is a copy of the last:

```
examples/NN-name/
  guest.c         the library — compiled to guest.cwasm, runs in the sandbox
  demo.c          an unmodified native program that links the .so
  functions.txt   stub manifest: one exported signature per line
  stub/           the cdylib crate -> libNAME.so (native symbols -> guest calls)
  Makefile        `include ../../common.mk`; sets LIB + a `check` target
```

Shared build logic is in [`common.mk`](common.mk); stub generation is in
[`tools/gen_stubs.sh`](tools/gen_stubs.sh).

## Working in an example

```bash
cd examples/01-scalars
make            # build everything — runs nothing (build and run can be on different machines)
make run        # run the demo against the wasm-sandboxed lib, via lind-wasm
make run-native # baseline: real native lib, no sandbox
make compare    # run native then sandboxed, back to back
make gen        # functions.txt -> stub/src/lib.rs  (committed; regenerate on change)
make check      # quick in-host smoke test (lind_run --call), no .so packaging
```

`make` only builds; running is explicit (`make run`) — running needs the full
Linux lind runtime, which the plain build does not.

Prerequisite: the toolchain built once from the repo root (`make build`).

### `make check` — the in-host debugging trick

Before packaging as a `.so`, you can exercise a guest export directly with the
`--call` flag, which runs a named export instead of `_start`:

```bash
./scripts/lind_run --call add examples/01-scalars/guest.cwasm 2 3   # -> [I32(5)]
```

This is the fastest way to confirm the guest side works in isolation from the
native linking / stub layer.

## Adding the next iteration

1. `cp -r examples/01-scalars examples/02-buffers`
2. Edit `guest.c`, `demo.c`, `functions.txt`; set a new `LIB` in the `Makefile`.
3. `make gen && make && make run` — for the scalar-only generator this is enough;
   a pointer-using function is where `gen_stubs.sh` grows to emit marshalling glue.
