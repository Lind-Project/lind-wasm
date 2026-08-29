# Example 06 — OpenBLAS as a sandboxed native `.so`

The first **real-world** library. An unmodified program links our native
`libopenblas.so` with the standard `-lopenblas` and calls `cblas_ddot`, `cblas_daxpy`,
… — but the actual BLAS runs as WebAssembly inside the lind/wasmtime sandbox.

This differs from the toy examples in three ways:

1. **The guest is prebuilt**, not a local `guest.c` — it's the real OpenBLAS wasm
   library (from `lind-wasm-apps/openblas`) with a shim linked in.
2. **A guest shim** (`openblas_lind_shim.c`) exports `guest_malloc`/`guest_free` plus a
   thin wrapper per BLAS call (`lind_cblas_ddot` → `cblas_ddot`), so the host only ever
   looks up symbols we defined — independent of OpenBLAS's own export/GOT behavior.
3. **Floating-point marshalling** — `f64` scalar args and `f64` returns (added to the
   engine as `Arg::F64`/`Arg::F32` + `call_f64`/`call_f32`). Array sizes
   (`1+(n-1)*|inc|`) are computed in the stub.

## Subset wrapped so far

| C symbol | contract exercised |
| --- | --- |
| `cblas_idamax` | array in, **int** return (works on the pre-f64 engine) |
| `cblas_ddot` | two arrays in, **f64** return |
| `cblas_daxpy` | **f64** scalar arg, x in, y **in/out** |

## Build & run

### 1. Build the guest module (OpenBLAS wasm + shim)

After a successful `compile_openblas.sh` run (which produced `libopenblas.a`), run the
helper — it links the shim into the OpenBLAS wasm dylink library and AOT-compiles it to
a `.cwasm`, staging it into `lindfs/`:

```bash
OPENBLAS_SRC=/path/to/OpenBLAS ./build_guest.sh
# -> lindfs/lib/libopenblas_lind.cwasm
```

`build_guest.sh` reuses the same toolchain as `compile_openblas.sh` and mirrors its
dylink→`add-export-tool`→`lind-wasm-opt --target=library`→`lind_compile --precompile-only`
chain, adding `openblas_lind_shim.c` to the `--whole-archive` link. The flag strings it
reconstructs are grouped at the top of the script; if your `compile_openblas.sh` differs,
make them match. Every step echoes its command.

**Alternative (manual):** add one file to `compile_openblas.sh` itself — in the `clang`
command that links the shared wasm (`-Wl,-shared … --whole-archive libopenblas.a`),
append `openblas_lind_shim.c -I"$OPENBLAS_SRC"` before `-o`. Everything downstream is
unchanged.

Either way, note the guest module's **host** path (e.g. `lindfs/lib/libopenblas_lind.cwasm`).

### 2. Build the native stub + demo, and run

```bash
make                                            # builds libopenblas.so (native) + demo
make run GUEST_CWASM=$LIND_WASM/lindfs/lib/libopenblas_lind.cwasm
```

`make run` sets `LIND_MODULE` (the guest), `LIND_PRELOAD` (libc/libm), and
`LIND_ENABLE_FPCAST=1` (OpenBLAS's dylink build is fpcast-emu — the runtime must match).

Expected:

```
cblas_idamax   -> 3 (want 3) OK
cblas_ddot     -> 300 (want 300) OK
cblas_daxpy    -> 120 (want 120) OK
               y = {12, 24, 36, 48}
all checks passed
```

## Verification gates (if `make run` fails)

Because this is the first real dylink guest, check these in order:

1. **Instantiation / fpcast.** If init fails, try toggling `FPCAST` (`make run
   FPCAST=0`) to match how the guest was compiled.
2. **Symbol resolution.** A `` `lind_cblas_ddot` not found `` panic means the shim
   wasn't linked into the guest (step 1) — confirm the `.cwasm` exports it.
3. **`guest_malloc` missing.** Same cause — the shim provides it; re-check the link.

## Next

`cblas_dgemv`, then `cblas_dgemm` (leading dimension `lda`, in/out `C`, β-read
semantics) — the stub computes `lda*cols`; no new engine capability needed. Then the
single-precision (`s*`) variants via `Arg::F32`/`call_f32`.
