# Example 05 — structs (const struct* input)

Adds a **`const struct *` input** argument. A struct is the first argument where the
*layout* has to cross the sandbox boundary — and the host and guest don't lay it out
the same way, so it can't be copied byte-for-byte.

```c
struct Person { int id; long score; char *name; };
```

| | `id` | `score` | `name` | size |
| --- | --- | --- | --- | --- |
| host (LP64) | 0 | 8 | 16 | 24 |
| guest (ILP32) | 0 | 4 | 8 | 12 |

`long` and pointers are 8 bytes on the host but 4 in the guest, so every field after
the first one shifts. The struct is marshalled **field by field**: the stub reads the
host-layout struct (`#[repr(C)]`), and the engine repacks it into the guest layout —
truncating the `long`, and copying the `char *name` string into guest memory
separately (a nested copy-in, like `02-strings`).

## Pieces

| File | Role |
| --- | --- |
| `guest.c` | `struct Person` + three functions, each isolating one field; uses libc |
| `demo.c` | unmodified native caller passing a `struct Person` by pointer |
| `functions.txt` | the `struct Person {...}` declaration + three `struct=Person` funcs |
| `stub/` | the cdylib → `libpersondemo.so` |

The marshalling lives in `lind-boot`'s `SandboxedLib::call` + the new `Arg::StructIn`
/ `Field` types (compute the guest layout, allocate, write each field at its guest
offset, nested-allocate pointers, free). The generator emits the `#[repr(C)]` mirror
and the `Field` array.

## Run

```bash
make gen        # functions.txt -> stub/src/lib.rs
make            # build everything (runs nothing)
make run        # run the wasm-sandboxed demo
make run-native # run the baseline (no sandbox)
```

Expected output (identical for `run` and `run-native`):

```
person_id      -> 42
person_score   -> 1000
person_namelen -> 12
```

`person_namelen` = 12 proves the `name` string crossed intact and was measured
*inside* the sandbox. libc (`strlen`/`malloc`) is preloaded — see the Makefile
`PRELOAD`.

## Still out of scope

Struct **output** / **return** / **by-value**, nested structs, arrays in structs.
This example proves **struct input**, field-by-field across ILP32/LP64.
