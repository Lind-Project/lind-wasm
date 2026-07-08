# Example 02 — strings (first marshalled argument)

The first function that crosses the guest memory boundary: `size_t str_len(const
char *)`, whose real implementation runs inside the sandbox. Builds on
[`01-scalars`](../01-scalars) by adding exactly one new mechanism — **copy-in
marshalling** of a pointer argument.

## Why a pointer can't just be passed through

The guest runs in its own wasm linear memory. A native `const char*` is a host
address that means nothing there — the guest would read whatever sits at that
offset in *its* memory. So the host stub must:

1. `guest_malloc(len+1)` — reserve space **inside** the guest,
2. copy the string bytes (including the trailing NUL) into guest memory,
3. call `str_len` with the resulting **guest offset**,
4. read back the length (a scalar — the easy half),
5. `guest_free` the copy.

The return value is a plain `size_t`, so nothing is marshalled on the way out.

## Pieces

| File | Role |
| --- | --- |
| `guest.c` | exports `str_len` **and** `guest_malloc`/`guest_free` so the host can place data into guest memory |
| `demo.c` | unmodified native caller; prints libc `strlen` alongside as a cross-check |
| `functions.txt` | `str_len usize cstr` — the `cstr` type triggers marshalling |
| `stub/` | the cdylib → `libstrdemo.so` |

The marshalling itself lives in `lind-boot`'s `SandboxedLib::call` + the `Arg`
enum (copy each `Arg::Buf` into the guest, pass its offset, free after). The
generator emits the stub that turns a host `*const c_char` into an `Arg::Buf`.

> The function is named `str_len`, **not** `strlen`, on purpose: the `.so` must not
> export a symbol that shadows libc's own `strlen` for the whole host process.

## Run

```bash
make gen        # functions.txt -> stub/src/lib.rs
make            # build everything (runs nothing)
make run        # run the wasm-sandboxed demo
make run-native # run the baseline (no sandbox)
```

Expected output (identical for `run` and `run-native`):

```
str_len("") = 0 (native strlen = 0)
str_len("hello") = 5 (native strlen = 5)
str_len("lind sandbox") = 12 (native strlen = 12)
```

There is no `make check`: `lind_run --call` only passes scalar `i32` args, so it
can't drive a pointer-taking export — that's exactly what the `.so` stub's
marshalling is for.

## Still out of scope

Out-params / filled buffers (host allocates, guest writes back), structs,
callbacks, and concurrency. This example only proves **copy-in of one buffer**.
