# marshal-infer — worked example (full pipeline)

This folder runs the **entire** inference pipeline on one tiny library so you can
read every intermediate artifact. Reproduce it all with:

```bash
tools/marshal-infer/build.sh         # build the tool once
tools/marshal-infer/example/run.sh   # regenerate every file below
```

Input and generated artifacts:

| file | what it is | stage |
|---|---|---|
| `demo.c` | the input library (7 functions, each exercising one feature) | — |
| `demo.ll` | the wasm32 LLVM IR **+ DWARF** that the tool consumes (readable form of `demo.bc`) | 1 |
| `demo.tree.txt` | the tool's intermediate inference, as a human-readable tree | 2 |
| `demo.marshal.json` | the final JSON sidecar (the delivery) | 3 |

The pipeline is three stages: **C → IR/DWARF → inference → JSON**.

---

## Stage 1 — C to wasm32 LLVM IR (+DWARF)

```
clang --target=wasm32-unknown-wasi --sysroot=<sysroot> -g -O1 -emit-llvm -c demo.c -o demo.bc
```

This is exactly what `lind-clang --emit-llvm` does. Three things matter:

- **`--target=wasm32-unknown-wasi`** → the IR is ILP32: pointers and `unsigned long`
  are **4 bytes**, struct layouts use wasm32 padding. Analyzing this build (not a
  native one) is why the offsets/sizes come out correct for the cage ABI.
- **`-g`** → DWARF debug metadata is embedded. We **need** it because clang-18 uses
  *opaque pointers*: in the IR a pointer is just `ptr`, with no pointee type. The
  shape of what a pointer points at lives only in DWARF.
- **`-O1`** → runs mem2reg so arguments and memory accesses are clean SSA (good for
  the read/write analysis) without inlining away the function boundaries.

Two kinds of signal live in `demo.ll`. **Behaviour**, in the IR instructions and
attributes — e.g. for `encode`:

```llvm
define hidden void @encode(ptr nocapture noundef writeonly %0,   ; dst — writeonly
                           ptr nocapture noundef readonly  %1,   ; src — readonly + (const in C)
                           i32 noundef %2) ... {                 ; n   — i32 (unsigned long = 4B!)
  ...
  %9  = load  i8, ptr %8           ; reads  through %1 (src)
  store i8 %10, ptr %11            ; writes through %0 (dst)
```

**Shape**, in the DWARF metadata — e.g. `struct buffer`:

```llvm
!74 = !DICompositeType(tag: DW_TAG_structure_type, name: "buffer", size: 64 ...)  ; 64 bits = 8 bytes
!76 = !DIDerivedType(tag: DW_TAG_member, name: "data", baseType: !77, size: 32)        ; offset 0
!78 = !DIDerivedType(tag: DW_TAG_member, name: "len",  baseType: !79, size: 32, offset: 32) ; offset 32b = 4B
```

---

## Stage 2 — inference (the tool)

`marshal-infer demo.bc` loads the module and, for each exported function, builds a
tree from DWARF and then annotates it from the IR. Output is `demo.tree.txt`.

### 2a. Parameter tree from DWARF — `src/ParamTree.cpp`

`buildFunctionTrees` reads the function's `DISubprogram` → `DISubroutineType` (the
ordered parameter type list) and unfolds each parameter type with
`buildTreeFromDIType` (`ParamTree.cpp:112`):

- a pointer (`DW_TAG_pointer_type`, `:134`) → one **pointee** child; the pointee
  type comes from DWARF, and `hasConstQual` (`:79`) records whether it was `const`.
- a struct (`DW_TAG_structure_type`) → one child **per `DW_TAG_member`**, copying
  each field's byte `offset` (`getOffsetInBits()/8`, `:158`) and size.
- everything else → a scalar leaf.

This alone produces the *structure* you see for `checksum`: a pointer to an
8-byte `buffer` struct whose fields are `data` @0 and `len` @4. No behaviour yet.

### 2b. Annotation from IR — `src/Infer.cpp`

`inferFunction` walks each pointer argument and fills in direction, size, handle,
and the return kind.

**Direction** — `analyzeAccess` (`Infer.cpp:141`) walks every value *derived* from
the argument (through GEPs/casts/phis) and records reads vs writes: a `load` whose
pointer is the arg ⇒ read; a `store` ⇒ write; known mem/string calls and memcpy
intrinsics contribute too. `directionFrom` (`:266`) maps read→IN, write→OUT,
both→INOUT. For `encode`: the `store i8 … ptr %0` makes **dst = OUT**, the
`load i8 … ptr %1` makes **src = IN**. Two refinements: a **`const`** pointee forces
IN (`:396`, this is why `demo_strlen`/`checksum` are IN even though glibc-style
loops can hide the reads), and if nothing is observed but the buffer is sized, we
fall back to the safe **INOUT + a warning**.

**Size** — for each pointer we look for a byte length: a real length operand from a
mem/str call → `from_arg`/`from_arg_pointee`; a `char*` used as a string (or with no
length companion) → `cstr`; a struct/scalar pointee → `const` `sizeof`; otherwise a
heuristic picks the most size_t-shaped sibling argument (`sizeyRank`, `:74`) and
emits a warning. For `encode`, `n` is the only size_t-ish arg, so both buffers get
`from_arg(2)` (flagged heuristic). For `demo_strlen`, the `char*` with no length
becomes `cstr`.

**Nested structs** — `annotateComposite` (`:209`) walks the struct's fields: marks
each `touched` (library-side we copy every accessed field — we lack the caller to do
KSplit's precise both-sides intersection), and sizes a pointer field from a
size-like **sibling field**. For `buffer`, `data` is paired with sibling field `len`
→ `from_arg(1)` (field index 1), exactly the in-struct (buf,len) idiom.

**Handles** — an opaque `void*` with no derivable length is treated as a handle
(`:345`/`:359`): never deep-copied, translated through a token table. We *bias to
handle* because mistaking a handle for a buffer corrupts memory, while the reverse
only loses an optimization. So `ctx_read`/`ctx_close`'s `void *c` → `HANDLE`.

**Return** — `ctx_open` returns `noalias ptr` (a fresh `malloc`) to an opaque object
⇒ `ret=handle` (`:442`). A pointer that traces back to an argument ⇒ `ptr_alias_arg`
(e.g. `memcpy` returning its dst); a fresh non-opaque pointer ⇒ `force_local`.

The intermediate result (`demo.tree.txt`), e.g.:

```
=== encode ===  ret=void
  arg0:  [PTR] dir=out size=from_arg(arg2)  -> [SCALAR] unsigned char
  arg1:  [PTR] dir=in  size=from_arg(arg2)  -> [SCALAR] unsigned char
  arg2:  [SCALAR] unsigned long
=== checksum === ret=scalar
  arg0:  [PTR] dir=in size=const(8) -> [STRUCT] buffer (8)
           .data [PTR] dir=in size=from_arg(arg1) -> char   (sized by sibling field 'len')
           .len  [SCALAR] unsigned int @4
=== ctx_read === ret=scalar
  arg0:  [PTR] dir=in size=unknown HANDLE[void]
```

---

## Stage 3 — JSON sidecar (the delivery)

`marshal-infer --json -o demo.marshal.json demo.bc` serialises the annotated trees.
The `checksum` record (abbreviated) — note every field traces to a stage-2 decision:

```json
{ "name": "checksum", "ret": {"kind": "scalar"},
  "args": [
    { "kind": "PTR", "dir": "in", "size_kind": "const", "const_size": 8,
      "pointee": [ { "kind": "STRUCT", "type": "buffer", "size": 8, "fields": [
        { "kind": "PTR", "field": "data", "offset": 0, "touched": true,
          "dir": "in", "size_kind": "from_arg", "size_arg": 1,
          "pointee": [ {"kind":"SCALAR","type":"char","size":1} ] },
        { "kind": "SCALAR", "field": "len", "offset": 4, "touched": true }
      ] } ] } ],
  "warnings": ["arg0 field 'data': nested pointer direction assumed IN"] }
```

### Where each JSON field comes from

| JSON field | meaning | source signal | code |
|---|---|---|---|
| `kind` | scalar / ptr / struct | DWARF type tag | `ParamTree.cpp:112` |
| `offset`, `size` | wasm32 layout | DWARF `getOffsetInBits/getSizeInBits` | `ParamTree.cpp:158` |
| `dir` | in / out / inout | IR load/store + `const` + attrs | `Infer.cpp:141,266,396` |
| `size_kind`/`size_arg` | how big the region is | length-operand / string-use / sibling | `Infer.cpp:296–388` |
| `touched` | marshal this field? | accessed (library-side: all) | `Infer.cpp:209` |
| `handle` | opaque token, don't copy | unsized `void*` / noalias ctor | `Infer.cpp:345,442` |
| `ret.kind` | return translation | alias-arg / noalias / opaque | `Infer.cpp:420–450` |
| `warnings` | residue to review | every uncertain decision | throughout |

---

## How this scales to the delivery

- **Any library, in the build:** `lind-clang --emit-marshal --compile-library foo.c`
  runs stages 1–3 and drops `foo.marshal.json` beside the artifact.
- **libc:** `tools/marshal-infer/infer_libc.sh` does this for every glibc TU and
  aggregates into `libc.marshal.json`, filtered to the symbols `libc.cwasm` exports
  (`marshal-infer --json --exports <list> *.bc`).

See `../README.md` and `plan-inference.md` / `impl-log-inference.md` at the repo root.
