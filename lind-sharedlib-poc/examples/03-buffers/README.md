# Example 03 — buffers (caller-allocated output buffers)

Builds on `02-strings` (copy-**in**) by adding **copy-out**: functions that write
into a buffer the *caller* allocates. The host stub allocates the buffer inside the
guest, runs the call, and copies the result back into the caller's buffer.

The four functions each demonstrate a different **copy-back-length contract** — how
many bytes the guest actually wrote. This can't be read off the C type (`char *out,
size_t n` only gives the *capacity*); it's a per-function fact declared in
`functions.txt`:

| Function | Contract | manifest `len=` |
| --- | --- | --- |
| `to_upper` | bytes written = the return value | `ret` |
| `greet` | out is a NUL-terminated C string | `nul` |
| `fill_pattern` | the whole buffer is filled | `cap` |
| `extract_word` | length reported via a `size_t*` out-param | `arg4` |

## Pieces

| File | Role |
| --- | --- |
| `guest.c` | the four functions + `guest_malloc`/`guest_free`; uses libc |
| `demo.c` | unmodified native caller exercising all four |
| `functions.txt` | the manifest, incl. `outbuf,cap=C,len=X` and `outlen` specs |
| `stub/` | the cdylib → `libbufdemo.so` |

The marshalling lives in `lind-boot`'s `SandboxedLib::call` + the `Arg`/`OutLen`
types (allocate out-buffers, invoke, read out-lengths, resolve each buffer's
copy-back length, copy out, free). The generator just builds the `Arg` array.

## Run

```bash
make gen        # functions.txt -> stub/src/lib.rs
make            # build everything (runs nothing)
make run        # run the wasm-sandboxed demo
make run-native # run the baseline (no sandbox)
```

Expected output (identical for `run` and `run-native`):

```
to_upper      -> "HELLO WORLD" (11 bytes)
greet         -> "Hello, lind!"
fill_pattern  -> "ABCDEFGHIJ"
extract_word  -> "sandboxed" (len=9)
```

libc (`snprintf`/`toupper`/`malloc`) is preloaded — see the Makefile `PRELOAD`.

## Still out of scope

In/out buffers that are also read by the guest, structs, callbacks, and
concurrency. This example proves **copy-out**, in all four length flavors.
