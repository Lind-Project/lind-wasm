# 05-structs — design: struct input (field-by-field marshalling)

This iteration adds a **`const struct *` input** argument. It builds on the buffer
iterations, but a struct is the first argument where *layout*, not just bytes, has to
cross the boundary — and the two sides don't agree on the layout.

## The core problem: host and guest lay structs out differently

The host is **LP64** (pointers and `long` are 8 bytes); the guest is **wasm32 /
ILP32** (pointers and `long` are 4 bytes). So for

```c
struct Person { int id; long score; char *name; };
```

the field offsets differ:

| | `id` | `score` | `name` | size |
| --- | --- | --- | --- | --- |
| host (LP64) | 0 | 8 | 16 | 24 |
| guest (ILP32) | 0 | 4 | 8 | 12 |

A byte-for-byte `memcpy` would put every field after the first `long`/pointer at the
wrong offset. And `name` is a *pointer* — meaningless in the guest's address space —
so its target string has to be copied into guest memory separately (a nested copy-in,
exactly like `02-strings`), with the resulting 4-byte guest offset stored in the field.

So a struct is marshalled **field by field**: read each field with the host layout,
write it at the guest offset, converting widths and nested-allocating pointers.

## Who does what

- **The stub** (generated) knows the *host* layout for free: it declares a matching
  `#[repr(C)] struct Person`, so rustc computes host offsets. It reads `p.id`,
  `p.score`, `p.name`, copies the `name` string into an owned buffer, and hands the
  fields to the engine as a `Field` array in declaration order.
- **The engine** (`SandboxedLib::call`) knows the *guest* layout: from the field
  kinds it computes the wasm32 offsets (`guest_struct_layout`), `guest_malloc`s the
  struct, writes each field at its offset — `Long` truncated 8→4, `Ptr` nested-copied
  and stored as a 4-byte offset — and passes the struct's guest offset as the arg.

Input only: the struct is not copied back.

## Flow of a struct call (`person_namelen(&p)`)

```
app: person_namelen(&p)      p = { id:42, score:1000, name:"Ada Lovelace" }
  │  native call → stub in libpersondemo.so
  ▼
STUB (generated)
  let s0 = &*a0;                             // host-layout view (#[repr(C)])
  let s0_name = cstr_bytes(s0.name);         // "Ada Lovelace\0"
  let f0 = [ Field::I32(s0.id),
             Field::Long(s0.score as i64),
             Field::Ptr(&s0_name) ];
  Arg::StructIn(&f0)
  ▼
SandboxedLib::call  (lock the global Mutex)
  Phase 1  guest layout of [I32, Long, Ptr] → offsets [0,4,8], size 12
           base = guest_malloc(12)
           write id    at base+0  (4 bytes)
           write score at base+4  (long 8→4, truncated)
           p_name = guest_malloc(13); copy "Ada Lovelace\0"
           write p_name at base+8 (the 4-byte guest offset)
           param = base
  Phase 2  invoke → guest strlen(p->name) = 12
  Phase 5  free base and p_name
  ▼
STUB: return 12
```

## Changes by file

### A. Marshalling engine — `lind-boot`

**`src/lind-boot/src/lind_wasmtime/sandboxed_lib.rs`**

- **`Arg` gains `StructIn(&[Field])`** — a `const struct *` input.
- **New `Field` enum** — `I32` (int/float), `I64` (int64/double), `Long` (host 8-byte
  `long`/`size_t` → guest 4-byte), `Ptr(&[u8])` (a pointer field: nested-allocated).
- **`call` Phase 1** gains a `StructIn` arm: compute the guest layout, allocate the
  struct, write each field at its guest offset (converting `Long`, nested-allocating
  `Ptr`), and free everything (struct + nested) afterward. Phases 3–4 skip it (input).
- **New helper `guest_struct_layout`** — computes wasm32 field offsets + struct size
  from the field kinds (C alignment rules).

**`mod.rs`, `lib.rs`** — also re-export `Field`.

### B. Code generation — `lind-sharedlib-poc/tools/gen_stubs.sh`

- New **`struct NAME { <ftype> <field>; ... }`** declaration syntax (ftypes: `i32`,
  `long`, `i64`, `cstr`), and a new arg spec **`struct=NAME`**.
- Emits a `#[repr(C)]` mirror of each declared struct, then for a `struct=` arg builds
  the `Field` array (nested `cstr_bytes` for `cstr` fields) and `Arg::StructIn`.
- Imports grow as needed (`Field`, and `core::ffi::{c_int, c_long, c_longlong}`).

### C. The example — `lind-sharedlib-poc/examples/05-structs/`

`guest.c` (`struct Person` + three field-isolating functions, uses libc), `demo.c`,
`functions.txt`, `Makefile` (`LIB := persondemo`, `PRELOAD` libc/libm), `stub/`
(cdylib → `libpersondemo.so`), and the generated `stub/src/lib.rs`.

## Run

```bash
make run          # wasm-sandboxed
make run-native   # baseline
```

```
person_id      -> 42
person_score   -> 1000
person_namelen -> 12
```

## Still out of scope

Struct **output** (`struct *` copy-back), struct **return** and **by-value** structs
(unpredictable wasm ABI lowering), nested structs, arrays inside structs.
