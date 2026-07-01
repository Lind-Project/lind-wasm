# Example 01 — scalars

The smallest end-to-end slice: a native `libadd_sub.so` whose `add`/`subtract`
symbols run their real implementations inside the lind/wasmtime sandbox, called
by an unmodified native `demo.c`.

Both functions are pure scalar `int(int,int)`, so **nothing crosses the guest
memory boundary** — a wasm `i32` *is* a native `int`. This isolates the runtime
+ packaging plumbing from marshalling, which later examples add.

## Pieces

| File | Role |
| --- | --- |
| `guest.c` | the library; exported to wasm, compiled to `guest.cwasm` (runs in the cage) |
| `demo.c` | unmodified native caller; links `-ladd_sub` |
| `functions.txt` | stub manifest (`add`/`subtract`, both scalar) |
| `stub/` | the cdylib crate → `libadd_sub.so`: native `add`/`subtract` symbols that forward into the guest |

The stub crate depends on `lind-boot` (`../../../../src/lind-boot`) and, through
it, the whole lind/wasmtime/rawposix/3i stack.

## Run

```bash
make gen        # functions.txt -> stub/src/lib.rs
make            # build everything (runs nothing)
make run        # run the wasm-sandboxed demo
make run-native # run the baseline (no sandbox)
make check      # in-host smoke test via lind_run --call
```

Expected output (identical for `run` and `run-native`):

```
add(2, 3)       = 5
subtract(10, 4) = 6
```

## Intentional simplifications

- **Scalars only** — no pointers, buffers, structs, callbacks, or threads.
- **No `__wasm_call_ctors`** — the guest has no `main`/`_start`; `add`/`subtract`
  touch no libc/global state, so skipping constructors is fine here.
- **Global lock** — a wasmtime `Store` is not `Sync`, so the resident instance
  lives behind a `Mutex` and calls are serialized.
- **No chroot** — a loaded library must not chroot its host process.
