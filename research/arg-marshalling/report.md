# Automatic Function-Argument Marshalling for Library Interposition
### A broad literature survey — how existing systems describe, infer, and marshal the arguments of an intercepted function call

*Prepared for the Lind-Wasm library-interposition effort, 2026-05-30.*
*Scope: ~25 systems across six communities. PDFs of open-access sources are in `papers/`; full bibliography with access status in `sources.md`; per-source extractions in `raw-notes.md`.*

---

## 1. Why this problem exists, and what "marshalling" actually means here

When you intercept a call to a library function and send it *somewhere else* — a remote
process (our `lind-remote-lib` RPC path) or a sibling cage behind a grate portal (our
`instance_dylink` portal path) — the callee no longer shares the caller's address space.
The raw machine-level arguments are just integers in registers. A `char *` and an `int`
look identical. To re-execute the call correctly on the other side you must, for every
argument, answer a sequence of questions:

1. **Is it a pointer at all, or a scalar?**
2. **If a pointer: how much memory does it reference?** (a fixed struct size, a
   length carried in another argument, a NUL terminator, a sentinel element…)
3. **Which direction does data flow?** in (copy before), out (copy after), in/out (both).
4. **Does the pointed-to memory itself contain pointers** that must be chased
   recursively (e.g. zlib's `z_stream`, whose fields point at the input buffer, output
   buffer, and an internal state object)?
5. **Is it an *opaque handle*** — a value the callee owns and the caller must never
   dereference (a `FILE *`, a CUDA device pointer, a Mach port) — in which case the
   *value* travels but the *pointee* must **not** be copied?
6. **Is it a *callback* / function pointer**, valid only in the caller's world?
7. **What side effects** (`errno`, internal library state, allocated resources) must be
   reflected back?

Every one of these is a distinct **marshalling rule**, and the literature is, at bottom, a
catalogue of (a) **languages for *declaring* these rules** and (b) **analyses for
*inferring* them**. Our current `ArgSpec` —

```rust
enum ArgSpec { Scalar, Ptr { direction: In | Out | InOut, size: Arg(n) | NullTerminated | SameAsPtrArg(n) } }
```

— is a small, hand-written instance of the *declarative* family (it covers questions 1–3
for flat buffers) and does nothing for questions 4–7 or for inference. The survey below
locates that design in the broader space and extracts what is transferable.

A useful framing the whole field agrees on: **marshalling correctness reduces to knowing
the *region* and *direction* of every transitively-reachable pointer, plus a special-case
list for things that must not be copied (handles, callbacks).** Everyone solves the same
problem; they differ in *who supplies the region/direction knowledge* (a human via an IDL,
or an analysis) and *how rich the type language is*.

---

## 2. A taxonomy of argument categories, distilled from the literature

Nearly every system, independently, converges on the same set of categories. The names
differ; the concepts are stable. This is the vocabulary our `ArgSpec` should aim to cover.

| # | Category | What it is | Canonical handling | Who names it |
|---|----------|-----------|--------------------|--------------|
| 1 | **Scalar / by-value** | integer, float, enum, fixed struct passed by value | copy the bytes | everyone |
| 2 | **Fixed-size pointee** | `struct stat *` — size known from the static type | deep-copy `sizeof(T)` | everyone |
| 3 | **C-string** | NUL-terminated `char *` | scan to `\0` | MIDL `[string]`, syzlang `string`, our `NullTerminated` |
| 4 | **Counted buffer** | `(ptr, len)` where another arg gives the element/byte count | copy `count` elements | MIDL `[size_is]/[length_is]`, syzlang `len[]/bytesize[]`, MIG `count`, our `Arg(n)` |
| 5 | **Sentinel-terminated array** | array ended by a zero/`{}` element (e.g. `pci_device_id[]`, `argv`) | scan for sentinel | KSplit, syzlang `array` |
| 6 | **Output parameter** | caller passes space, callee fills it | allocate, copy back (no copy in) | MIDL `[out]`, SWIG `OUTPUT`/`argout`, our `Out` |
| 7 | **In/out parameter** | read and written | copy both ways | MIDL `[in,out]`, syzlang `inout`, our `InOut` |
| 8 | **Pointer-to-pointer / out-handle** | `T **` where callee allocates and returns size+buffer | two-level marshal; size known only after call | MIDL `Proc7` `[out, size_is(,*p)]`, `cudaMalloc` |
| 9 | **Nested / transitive struct** | a struct whose fields are themselves pointers (recursive, cyclic) | chase pointers to a fixpoint with a visited-set | KSplit "recursive projections", PtrSplit parameter trees, Sandcrust serde, MIDL `[ref]` graphs |
| 10 | **Tagged / discriminated union** | active field selected by a tag | a discriminator decides which arm to copy | MIDL `[switch_is]/[switch_type]`, KSplit discriminator fn |
| 11 | **Opaque handle / capability** | value the callee owns; caller must not dereference (`FILE*`, GPU ptr, Mach port, fd) | **pass the value through a translation table; do NOT copy pointee** | Cricket/GVirtuS/rCUDA handle maps, Mach port rights, syzlang `resource`, LXFI capabilities |
| 12 | **Callback / function pointer** | code valid only in caller's world | reverse-RPC / trampoline / forbid | RLBox callback registration; flagged out-of-scope by most |
| 13 | **Type-polymorphic `void *`** | type known only at a call site / from a tag | resolve via call-site analysis or refuse | KSplit (CCured "wild"), Microdrivers |
| 14 | **Special memory** | user vs kernel memory, device MMIO, shared/pinned memory | region-specific copy or map | KSplit `user` attribute, GVirtuS pinned/UVA |

The single most important *conceptual* split is **category 9 vs 11**: a `T *` that must be
**deep-copied** versus a `T *` that must be **passed by value as an opaque token**. Getting
this wrong is the difference between "works" and "corrupts memory / leaks a remote address."
Our current `ArgSpec` has no representation for category 11 at all.

---

## 3. Two cross-system comparison tables

### 3a. Which argument categories each system handles

Legend: ● first-class · ◐ partial / via escape hatch · ○ not handled / out of scope.
Categories keyed to §2.

| System (cluster) | 3 str | 4 buf+len | 5 sentinel | 6/7 out,inout | 8 ptr-to-ptr | 9 nested/recursive | 10 union | 11 opaque handle | 12 callback |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| **MS RPC / MIDL** (IDL) | ● | ● | ◐ | ● | ● | ● | ● | ◐ (context handle) | ○ |
| **Sun RPC / XDR / rpcgen** (IDL) | ● | ● | ◐ | ◐ | ● | ● (recursive XDR) | ● | ○ | ○ |
| **Mach MIG** (IDL) | ● | ● | ○ | ● | ◐ | ○ | ◐ | ● (port rights) | ○ |
| **gRPC / Protobuf** (IDL/schema) | ● | ● | n/a | n/a | n/a | ● (messages) | ● (oneof) | ○ | ○ |
| **syzkaller syzlang** (desc. lang) | ● | ● | ● | ● | ● | ● | ● | ● (`resource`) | ○ |
| **SWIG typemaps** (FFI) | ● | ● (multi-arg) | ◐ | ● (`argout`) | ◐ | ◐ (manual) | ◐ | ◐ (opaque ptr type) | ◐ |
| **ctypes / cffi** (FFI) | ● | ◐ | ○ | ● | ◐ | ◐ | ● | ● (`c_void_p`) | ● |
| **RLBox** (sandbox) | ● | ● | ◐ | ● | ◐ | ◐ (`tainted` deep) | ○ | ● (verify-on-use) | ● (registered) |
| **Sandcrust** (sandbox) | ● | ● | ◐ | ● | ◐ | ● (serde) | ◐ | ◐ | ○ |
| **LXFI** (sandbox) | — | — | — | ◐ | — | ◐ | — | ● (capabilities) | ◐ |
| **PtrSplit** (partition) | ● | ● | ◐ | ● (inferred) | ● | ● (parameter trees) | ◐ | ◐ | ○ |
| **KSplit** (partition) | ● | ● | ● | ● (inferred) | ● | ● (fixpoint) | ● (discriminator) | ◐ (resolve `void*`) | ○ |
| **Glamdring** (partition) | ◐ | ◐ | ○ | ◐ | ◐ | ◐ | ○ | ◐ | ○ |
| **rCUDA / GVirtuS / Cricket** (remoting) | ● | ● (`cudaMemcpy`) | ○ | ● | ● (out-handle) | ○ (avoided) | ○ | ● (handle map) | ◐ |

### 3b. *How* the rules are obtained — declared vs inferred — and the signature mechanism

| System | Rules come from… | Signature mechanism | Auto? |
|---|---|---|---|
| MIDL / DCE RPC | Hand-written `.idl` attributes | `[in][out][size_is][length_is][string][switch_is]`, 3 pointer kinds (`ref`/`unique`/`ptr`) | Declared |
| Sun XDR / rpcgen | Hand-written `.x` spec | XDR type language; `xdr_*` (de)serializers generated; recursive types allowed | Declared |
| Mach MIG | Hand-written `.defs` | routine params tagged `in/out/inout`; **out-of-line (ool)** vs inline; **port rights** as typed descriptors | Declared |
| Protobuf / gRPC | Hand-written `.proto` | message schema + varint/length-delimited wire format; `oneof` for unions | Declared |
| syzlang | Hand-written (partly auto-extracted from headers) | `ptr[dir,T]`, `buffer[dir]`, `array`, `string`, `len[f]`, `bytesize[f]`, `resource` | Declared (+tooling) |
| SWIG | Library of reusable **typemaps** matched by type+name | `%typemap(in/out/argout/check/freearg)`, `numinputs=0`, multi-arg typemaps | Declared (reusable) |
| ctypes / cffi | Programmer states `argtypes`/`restype`; cffi can **parse C headers** | `POINTER`, `c_char_p`, `byref`, `CFUNCTYPE`; cffi reads declarations | Declared (cffi semi-auto) |
| RLBox | **Type system** forces handling at the boundary | `tainted<T>` types; data from sandbox is tainted until verified; swizzling of pointers | Compiler-enforced |
| Sandcrust | Programmer annotates the function; macros generate (de)serialization | Rust `serde` serialization of all args/returns over a pipe | Declared (1 macro) |
| LXFI | Programmer writes **API integrity annotations** | capabilities + `copy`/`transfer` annotations on module interfaces | Declared |
| PtrSplit | **Static analysis** | PDG + **parameter trees**; **selective pointer-bounds tracking** at runtime to get sizes | **Inferred** |
| KSplit | **Static analysis** (+small manual residue) | **projections** (field subset) + CCured/NesCheck pointer classes + def-use & call-site inference; warns on residue | **Inferred** |
| Glamdring | **Static analysis** from security annotations | data-flow from a few `sensitive` source annotations; partition + generated marshalling | **Inferred** (seeded) |
| rCUDA / GVirtuS / Cricket | Hand-written per-API stubs (Cricket uses rpcgen) | **shadow handle table** mapping local↔remote pointers; explicit copy only for `cudaMemcpy` | Declared (per-API) |

---

## 4. Per-cluster narrative

### 4.1 IDL / RPC annotation languages — the mature, *declarative* answer

This is the oldest and most complete vocabulary, and it is essentially a superset of our
`ArgSpec`. The richest is **Microsoft RPC / MIDL** (and its DCE RPC ancestor):

- **Direction**: `[in]`, `[out]`, `[in,out]` — exactly our `direction`.
- **Size**: `[size_is(n)]` gives *allocated* element count; `[length_is(n)]` gives
  *transmitted* count; `[max_is]`/`[first_is]`/`[last_is]` give bounds. Crucially these
  reference **another parameter** — `[in] short m; [in, size_is(m)] short a[]` — which is
  precisely our `Arg(n)`. ([MIDL `size_is`](https://learn.microsoft.com/en-us/windows/win32/midl/size-is))
- **Strings**: `[string]` ⇒ size taken from the NUL terminator (our `NullTerminated`).
- **Pointer-to-pointer & runtime-sized out-arrays**: MIDL's `Proc7` example,
  `[out, size_is(,*pSize)] my_type **ppMyType`, encodes "callee allocates an array whose
  size is unknown until the call returns, and reports it through `*pSize`." This is the
  `cudaMalloc`/getline pattern — **category 8** — which our `ArgSpec` cannot express.
- **Pointer *kinds*** (the subtlest idea, worth stealing): `[ref]` (always non-null, no
  aliasing — cheap), `[unique]` (may be null, no aliasing), `[ptr]`/full (may alias and
  form cyclic graphs — needs aliasing-aware marshalling that detects shared/cyclic
  pointees). The kind tells the marshaller *how hard* it must work on category-9 graphs.
- **Unions**: `[switch_is(tag)]` + `[switch_type]` names the discriminator field — our
  category 10.

**Sun RPC / XDR** ([RFC 4506](https://www.rfc-editor.org/rfc/rfc4506)) takes the same
declarative route but as a *data* language: you write a `.x` file, `rpcgen` emits paired
`xdr_T()` routines that both serialize and deserialize (one function, two directions),
and XDR supports recursive types (linked lists) directly. This "one (de)serializer per
type, composed structurally" design is what **Cricket** reuses via TI-RPC to remote CUDA.

**Mach MIG** contributes two ideas the others lack. First, the **out-of-line (ool)**
distinction: small data travels *inline* in the message; large buffers travel *ool* and
are page-mapped (copy-on-write) rather than byte-copied — a size-driven transport choice
that maps onto our future `shmem://` idea. Second, **port rights** are first-class typed
descriptors: a Mach message can carry a *capability* (send/receive right) which the kernel
translates between the sender's and receiver's port name spaces. That is the textbook
**opaque-handle-translation** mechanism (category 11), done in the kernel.

**Protobuf/gRPC, Thrift, Cap'n Proto, FlatBuffers** are the modern descendants but solve a
*narrower* problem: they marshal *self-describing message trees*, not arbitrary C pointer
graphs. There is no `[in]/[out]` (everything is a value), no aliasing, no opaque handles.
The lesson they carry is about *wire format*, not *category inference*: length-delimited
fields, `oneof` for unions, and (Cap'n Proto/FlatBuffers) zero-copy arena layouts.

### 4.2 Syscall / API description languages — the same vocabulary, machine-checkable

**syzkaller's syzlang** is the most `ArgSpec`-like artifact in the entire survey and the
best single template to copy. It describes each syscall argument with composable type
constructors: `ptr[in|out|inout, T]`, `buffer[dir]`, `array[T]` (fixed or ranged),
`string`/`stringnoz`, integer ranges and `flags`, **`len[field]` / `bytesize[field]`**
(a length field bound to *another* field — our `Arg(n)`, but symbolic and bidirectional),
per-field direction inside structs, conditional fields `(if[expr])` for variant layouts,
and — importantly — **`resource`**, an opaque handle type that models fds and other tokens
and tracks producer→consumer dataflow across calls. syzlang shows that one small, regular
type algebra can express categories 1–11 cleanly; descriptions are partly auto-extracted
from kernel headers (`syz-extract`/`syzgen`). ([syzlang syntax](https://github.com/google/syzkaller/blob/master/docs/syscall_descriptions_syntax.md))

`strace`/`ltrace` make the same point pragmatically: they carry a hand-written decoder per
syscall/prototype that knows each arg's category (string, buffer+len, struct, flags) — a
reminder that *somebody* always encodes this knowledge; the only question is whether it is
hand-written or inferred.

### 4.3 FFI / binding generators — engineering patterns for "each category differs"

**SWIG typemaps** are the cleanest engineering expression of the core difficulty. A typemap
is a reusable rule attached to a (type, name) pattern, split across the call lifecycle:
`in` (target→C), `check` (validate), `out` (C return→target), **`argout`** (copy out-params
back), `freearg` (cleanup). Two patterns are directly relevant:

- **Out-parameter**: `%typemap(in, numinputs=0) int *OUTPUT (int tmp) { $1=&tmp; }` plus an
  `argout` that returns `tmp` — i.e. "this pointer takes no input; allocate, run, copy back"
  (our `Out`). ([SWIG typemaps](https://www.swig.org/Doc4.0/Typemaps.html))
- **Multi-argument typemap**: `%typemap(in) (char *buf, int len) { … }` *bundles a
  consecutive `(pointer,length)` pair into one logical argument.* This is the declarative
  form of our `SameAsPtrArg`/`Arg(n)` coupling, and it generalizes: the size rule lives in
  a rule that *spans two physical arguments.*

`ctypes`/`cffi` reinforce that the programmer (or a header parser, in cffi's case) supplies
`argtypes`; `c_char_p` vs `c_void_p` vs `POINTER(T)` vs `CFUNCTYPE` *is* the category tag,
and `byref` marks out/inout. Rust's `bindgen` shows the limit of pure type translation: it
faithfully reproduces C signatures but **cannot recover size/direction semantics** (it
emits raw `*mut T`), which is exactly why the partitioning systems below need analysis.

### 4.4 Library / driver sandboxing — correctness of the boundary, not just the copy

These systems care less about a rich category language and more about *making the boundary
sound*, which surfaces the hard semantic issues (categories 9, 11, 12 and side effects).

- **RLBox** (USENIX Sec'20) is the influential one. Rather than infer marshalling, it
  changes the *type system*: every value coming from the sandbox is `tainted<T>`, and the
  C++ compiler **refuses to use it** until you explicitly `verify()`/`copy_and_verify()` it
  across the boundary. Pointers are *swizzled* between the app's and sandbox's address
  spaces. Callbacks must be explicitly *registered*. The lesson: you don't have to
  auto-marshal everything if the type system *forces a human to handle each boundary value
  exactly once*, turning silent memory bugs into compile errors. ([RLBox](papers/rlbox_sec20.pdf))
- **Sandcrust** (PLOS'17) is the pragmatic extreme: annotate a function with one macro, and
  it **`serde`-serializes all arguments and return values** over a pipe to a forked sandbox
  process. Deep/transitive copy "for free" via a generic serializer — at the cost of no
  sharing and full-copy overhead. Directly analogous to "what if our portal just serialized
  everything?" ([Sandcrust](papers/sandcrust_plos17.pdf))
- **LXFI** (SOSP'11) isolates kernel modules using **capabilities** plus interface
  annotations stating which references are transferred/copied — an explicit, manual version
  of categories 9/11 that KSplit later set out to automate. ([LXFI](papers/lxfi_sosp11.pdf))
- **Cali** (AsiaCCS'21) automatically isolates a library into its own process using a
  **Program Dependence Graph** at link time to compute what memory is shared — the
  user-space sibling of KSplit (full text in the Bauer dissertation, `papers/bauer_thesis_cali.pdf`).
- **XFI / Wedge / Privtrans** are the classics establishing the *boundary* but leaving
  marshalling largely manual (Privtrans inserts `send/recv` calls at a programmer-marked
  split; Wedge's `sthreads` define memory sharing by tags). ([XFI](papers/xfi_osdi06.pdf), [Wedge](papers/wedge_nsdi08.pdf), [Privtrans](papers/privtrans_sec04.pdf))
- **Donky / Hodor / ERIM** are *enforcement* mechanisms (MPK/intra-process isolation); they
  are about *how cheaply you cross* the boundary, not what to copy — relevant to our cage
  cost model but not to argument categories. ([Donky](papers/donky_sec20.pdf), [Hodor](papers/hodor_atc19.pdf))

### 4.5 Automatic partitioning & inference — the layer-1 answer we most want

This cluster is the heart of "can we *infer* the rules?" and the answer is a qualified yes.

- **PtrSplit** (CCS'17, Liu–Tan–Jaeger) is the foundational result for *general pointers*.
  It builds a **Program Dependence Graph**, represents each parameter as a **parameter
  tree** (a structural unfolding of a pointer type into its transitively-reachable
  components — i.e. an automatically-derived category-9 deep-copy plan), and, for sizes it
  cannot get statically, uses **selective pointer-bounds tracking**: a lightweight runtime
  instrumentation that carries each pointer's bounds so the marshaller knows exactly how
  many bytes to copy. It is *modular* (no whole-program pointer analysis). The two
  takeaways: (1) a *parameter tree* is the right data structure for transitive marshalling;
  (2) **when static size inference fails, fall back to runtime bounds metadata** rather than
  giving up. (Full treatment in `papers/SHEN_LIU_Dissertation.pdf`; slides in
  `papers/ptrsplit_ccs17.pdf`.)
- **KSplit** (OSDI'22) is the most directly applicable paper in the survey — it automates
  exactly our layer-1 for the hardest real target (Linux drivers ↔ kernel), and its
  taxonomy of "ambiguous idioms" *is* our category list. Mechanisms worth copying verbatim:
  - **Projections**: for a struct argument, the generated IDL lists *only the fields the
    callee actually reads/writes* (computed by a field-sensitive **parameter-access
    analysis**). Don't copy the whole `z_stream`; copy the fields that matter.
  - **Direction inference for free**: the same access analysis records READ vs WRITE per
    field, which *is* the in/out/inout decision — no human needed.
  - **Pointer classification via CCured/NesCheck**: every pointer is *safe* (single
    object ⇒ category 2), *sequential* (arithmetic ⇒ array ⇒ categories 4/5), or *wild*
    (involved in casts ⇒ ambiguous `void*` ⇒ category 13). Cheap and surprisingly decisive.
  - **String/array disambiguation from *uses***: if a `char*`'s aliases flow into
    `strlen`/`strcpy`, it's a string; def-use chains + call-site usage settle most cases.
  - **Unions**: reconstruct lost field names by matching IR access offsets to struct
    layout, then require a **discriminator function** (= MIDL `switch_is`) to pick the arm.
  - **Recursive structures**: emit a self-referential projection; the generated glue
    **traverses pointers to a fixpoint with a visited-set** (handles cycles) — the correct
    category-9 algorithm.
  - **Opaque/error pointers, user memory, container-of**: detected and either special-cased
    or turned into a **specific warning** for a human.
  - Result on `ixgbe`: 2,476 lines of interface spec generated, only ~53 lines of manual
    fixups. The design philosophy — *infer the common case, emit a precise warning for the
    residue* — is the realistic target for us. ([KSplit](papers/ksplit_osdi22.pdf))
- **Glamdring** (ATC'17) infers a partition + its marshalling from a *few* programmer
  `sensitive` source/sink annotations via data-flow analysis — the "seed a little, infer the
  rest" model. ([Glamdring](papers/glamdring_atc17.pdf)) **Program-mandering** (CCS'19) adds
  a *quantitative* angle: choose the cut that minimizes boundary-crossing/marshalling cost
  subject to a security goal. ([Program-mandering](papers/program-mandering-ccs2019.pdf))

### 4.6 Accelerator / GPU API remoting — opaque handles done at scale

rCUDA, GVirtuS, and Cricket remote the CUDA API over a network and therefore confront
categories 8 and 11 head-on, with a consistent answer:

- **Opaque handles are passed by value, never dereferenced.** Cricket states it plainly:
  for device pointers and internal-resource pointers it "passes only the raw pointer
  values, ignoring the fact that they reference address spaces in a different process,"
  copying real data only for the explicit `cudaMemcpy(ptr, len)` family. ([Cricket](papers/cricket_cpe22.pdf))
- **A shadow handle table reconciles the two address spaces.** GVirtuS keeps a map of every
  device/managed pointer (with its size and a host shadow under Unified Virtual Addressing),
  and classifies a pointer as host-vs-device by *looking it up in the table* — i.e. category
  membership is decided by a runtime registry, not by the static type. ([GVirtuS](papers/gvirtus_arm_2016.pdf))
  rCUDA similarly "assigns identifiers to pointers" and packs the identifier, not the data. ([rCUDA](papers/rcuda_aaas_arxiv.pdf))
- **Stubs are co-designed per API** (Cricket generates them with rpcgen/TI-RPC; GVirtuS uses
  a Frontend/Backend/Handler triple per function) — confirming that at the API granularity,
  the *declarative* route is still the norm even in 2022, because GPU drivers are closed and
  not analyzable.
- The 2024 characterization paper quantifies the cost that drives all of this:
  serialization/deserialization of arguments is a first-order latency term, which is why
  systems avoid copying handle pointees and reach for shared memory/RDMA for big buffers. ([GPU API remoting](papers/2401.13354_gpu_api_remoting_network.pdf))

---

## 5. Recurring challenges and how the field solves them

| Challenge | Why it's hard | Solutions seen, best-first |
|---|---|---|
| **Knowing pointee size** | a bare pointer carries no length | declared `[size_is]`/`len[]` (MIDL, syzlang); inferred via PDG + parameter trees (PtrSplit); **runtime pointer-bounds metadata** when static fails (PtrSplit); NUL/sentinel scan for strings/arrays (everyone) |
| **In/out/inout direction** | over-copying is slow, under-copying is wrong | declared attributes; **inferred from per-field READ/WRITE access analysis** (KSplit) — the cleanest automatic answer |
| **Transitive / nested / cyclic structs** (the `z_stream` case) | naive deep copy loops forever; over-copy wastes work | **projection** = copy only accessed fields (KSplit); **parameter tree** plan (PtrSplit); **fixpoint traversal with visited-set** for cycles (KSplit); generic **serde** if you don't mind full copy (Sandcrust); pointer *kinds* tell you whether aliasing is possible (MIDL `ref`/`unique`/`full`) |
| **Opaque handles** | must travel as value; copying pointee is wrong/impossible | **shadow handle table** local↔remote (rCUDA/GVirtuS/Cricket); **kernel-translated capabilities** (Mach port rights, LXFI); typed `resource` (syzlang); MIDL **context handles** |
| **Pointer/address validity across spaces** | a remote address is meaningless locally | **swizzling** (RLBox); identifier indirection (rCUDA); never expose raw remote pointers (Cricket) |
| **Callbacks / function pointers** | code lives only in caller | **register + reverse-RPC trampoline** (RLBox); most systems declare it out of scope (incl. our current design) |
| **TOCTOU / double-fetch** | shared memory can change between fetch and use; a buffer read twice may differ | **copy once into private memory then operate** (the hardware/copy-based driver isolation model in KSplit); RLBox `tainted` forces a single verified read; (the SFI single-copy model trades this for per-access checks) |
| **`errno` & side effects** | live in the callee's world | ship `errno` in every response and write it back (our design already does this; MIG/RPC ship status); route stateful libs (strtok/rand) consistently to one instance |
| **Object lifetime / ownership** | who frees, and when | **hybrid static+dynamic tracking** of alloc/free across the boundary (KSplit); ownership annotations (LXFI `transfer`); arena/lifetime types (RLBox) |
| **Unions / polymorphic `void*`** | active type is dynamic | discriminator function / `switch_is` (KSplit, MIDL); resolve `void*` by call-site cast analysis, else warn (KSplit) |
| **Residual ambiguity** | static analysis can't always decide | **infer the common case, emit a precise per-site warning** for a human (KSplit's defining principle) — far better than silent wrong copies |

A meta-point: **declarative IDLs are complete but manual; static inference is automatic but
incomplete.** No system is fully automatic *and* fully general. The state of the art
(KSplit) is "≈98% inferred, the rest flagged" — and that is the realistic bar for us.

---

## 6. Lessons for our design (retargeted to `new-design.md`: library-level 3i)

The new direction (`new-design.md`) drops the **config file** but keeps **declarative
marshalling metadata** — it moves into `struct lind_abi_spec`, supplied by *untrusted grate
code* at `register_lib_handler` time and consumed by the *trusted* dispatch/transfer
mechanism. So `lind_abi_spec` **is** an in-code marshalling IDL of the same family as MIDL,
syzlang, and XDR; every taxonomy lesson below targets it, not a config schema. The added
constraint is that **one model must serve inter-cage, inter-process, and inter-machine
placement uniformly.**

### 6.1 Layer the marshaller: transport-agnostic *shape* vs transport-specific *movement*

This is the architectural key to a *universal* model, and the literature is unanimous on it.
The **region + direction + transitive-copy plan** for a call's arguments is **identical**
across all three placements — it is a pure function of the function signature (PtrSplit's
*parameter tree*, KSplit's *projection*). Only two things vary by transport:

- **the copy primitive** — cross-cage `memcpy` ↔ socket (de)serialize ↔ shared-memory/RDMA;
- **handle / address translation** — see §6.3.

**Mach MIG is the exact precedent**: one `.defs` spec drives the same stub, while the kernel
realizes out-of-line memory mapping and port-right translation differently per case.
Cricket/GVirtuS likewise keep one rpcgen-generated stub and swap unix/tcp/shmem/RDMA
underneath. So `lind_abi_spec` should describe the data *shape* **once**; a thin trusted
`transfer` primitive plus the untrusted handler choose the *mechanism*. This layering is also
what lets "network transport live in user code" (Open Q12) be true without duplicating the
marshalling logic three times.

### 6.2 Place each marshalling concern on the right side of the TCB

`new-design.md` §8 wants policy untrusted and only minimal mechanism trusted. Mapping each
marshalling concern to a side, guided by the literature:

| Concern | Side | Why / precedent |
|---|---|---|
| Routing, placement, transport choice, caching/deny | **Untrusted** handler | the whole point of library-level 3i; RLBox/grate philosophy |
| Flat region copy with bounds + cage validation | **Trusted** | a capability-safety operation (`safe_copy`); §8.1 already lists it |
| **Handle / capability translation** | **Trusted** | a handle *is* a capability; Mach translates port rights in-kernel; a forged one breaks isolation |
| **Transitive / nested-struct traversal** (the `z_stream` walk) | **Untrusted, compiler-generated glue** | keeps TCB minimal (§4.1); KSplit emits this as generated glue, not kernel code |
| ABI descriptor (`lind_abi_spec`) authoring | **Untrusted** (or auto-generated, §6.5) | declared with the handler; trusted side only *interprets* flat ops |

Decisive recommendation: **the trusted side offers exactly two primitives — flat
`safe_copy(region, dir)` and `translate_handle(class, token)` — and never recurses.** The
untrusted (ideally auto-generated) glue walks the parameter tree and issues a sequence of
those primitives. This supports nested `z_stream`-style structs while keeping the trusted
transitive-copy surface at zero.

### 6.3 Opaque handles: the one category that breaks the universal symmetry

A `FILE*`, an fd, a GPU handle, or a raw pointer into the library-host cage is meaningless
across **every** boundary, so it must travel as a *token* and never be dereferenced on the
far side (Cricket states this explicitly; rCUDA/GVirtuS keep a **shadow handle table**).
Inter-cage this is precisely **Mach port-right translation** — a capability operation — so
the table belongs in the **trusted** runtime, per §6.2. `lind_abi_spec` has **no handle type
today**; adding `LIND_ABI_HANDLE` (carrying a handle-class id) is the **single
highest-value change**, because misclassifying a handle as a deep-copy pointer corrupts
memory. Default rule when unsure: treat an unknown pointer as a handle + force Local
execution, and warn — never silently deep-copy.

### 6.4 Extend `lind_abi_spec` toward the converged taxonomy

Today's `lind_arg_spec` (type ∈ {VOID,I32,I64,F32,F64,PTR}; `ptr_direction`; `size_kind` ∈
{NONE,CONST,FROM_ARG_VALUE,FROM_ARG_POINTEE,CSTR}) already covers categories 1–7 — including
`compress2`'s `FROM_ARG_POINTEE` out-length. The missing, high-value additions are:

1. **`OpaqueHandle` (category 11) — the most important gap.** Add a variant meaning "pass
   the value through a translation table; never dereference." This is what makes `FILE*`,
   fds, GPU-style handles, and the *internal-state pointer inside `z_stream`* correct. Back
   it with a **shadow handle table** per remote/cage endpoint (rCUDA/GVirtuS/Cricket model),
   keyed by the local token, storing the remote token (and vice-versa). Mach port-right
   translation and syzlang `resource` are the design references.
2. **Nested/transitive marshalling via a *projection tree* (categories 8–9).** Replace the
   flat `Ptr` with a recursive spec: a pointee is described by a struct layout whose fields
   are themselves `ArgSpec`s, marshalled by **fixpoint traversal with a visited-set** to
   handle cycles. This is the only way to do `z_stream` correctly. Borrow KSplit's
   **projection** optimization: record *which fields are actually touched* and copy only
   those. Borrow MIDL **pointer kinds** (`ref`/`unique`/`full`) to know when aliasing/cycle
   detection is even needed.
3. **Two-level / out-handle sizing (category 8).** Add a size spec for "callee allocates;
   true size reported via `*out_arg` after the call" — MIDL's `size_is(,*pSize)`. Needed for
   any `getline`/`cudaMalloc`-shaped function.
4. **Sentinel-terminated arrays (category 5)** — `Sentinel(value)` alongside
   `NullTerminated` (covers `argv`, `pci_id_table`-style tables).
5. **Tagged unions (category 10)** — a `Union{discriminator: ArgRef, arms: …}` spec, exactly
   MIDL `switch_is` / KSplit's discriminator function.
6. **Callbacks (category 12)** — at minimum a `Callback` variant that the scheduler *refuses
   to send remote* (forces Local), with reverse-RPC as a documented future extension
   (RLBox's registration model).
7. Keep the symbolic, *bidirectional* `len[field]` idea from syzlang: a size that can be an
   input (copy-in count) *or* an output (`length_is` after the call), not just a fixed input
   index.

Concretely, in the design's own C idiom, `lind_arg_spec` grows from a flat record into a
*recursive* one (the syzlang/MIDL shape) — but note the recursion is interpreted only by
untrusted glue (§6.2), so the trusted dispatcher still sees flat ops:

```c
enum lind_abi_type {
    LIND_ABI_VOID, LIND_ABI_I32, LIND_ABI_I64, LIND_ABI_F32, LIND_ABI_F64,
    LIND_ABI_PTR,                 // existing: pointer to a described layout
    LIND_ABI_HANDLE,              // NEW (cat 11): pass value as token, never deref
    LIND_ABI_CALLBACK,            // NEW (cat 12): reverse-portal / forbid-remote
};

enum lind_size_kind {
    LIND_SIZE_NONE, LIND_SIZE_CONST,
    LIND_SIZE_FROM_ARG_VALUE,     // (buf,len): len in arg i               [have]
    LIND_SIZE_FROM_ARG_POINTEE,   // size in *arg i (e.g. compress2)       [have]
    LIND_SIZE_CSTR,               // NUL-terminated                        [have]
    LIND_SIZE_SENTINEL,           // NEW (cat 5): array ended by sentinel elem (argv, id tables)
    LIND_SIZE_FROM_ARG_AFTER_CALL,// NEW (cat 8): callee fills *arg i with true size (MIDL size_is(,*p))
};

enum lind_ptr_kind { LIND_PTR_REF, LIND_PTR_UNIQUE, LIND_PTR_FULL }; // NEW (MIDL): cycle/alias hint

struct lind_arg_spec {
    enum lind_abi_type    type;
    enum lind_ptr_direction ptr_direction;
    enum lind_size_kind   size_kind;
    uint64_t              const_size;
    uint32_t              size_arg_index;
    enum lind_ptr_kind    ptr_kind;        // NEW: REF/UNIQUE => no cycle check; FULL => visited-set
    uint32_t              handle_class;     // NEW: index into the trusted handle table (LIND_ABI_HANDLE)
    const struct lind_layout *pointee;      // NEW: recursive layout for nested structs/unions
    uint32_t              flags;
};

// NEW: describes what a pointer points AT, enabling transitive copy + projections + unions
struct lind_layout {
    enum { LIND_LAYOUT_BYTES, LIND_LAYOUT_STRUCT, LIND_LAYOUT_UNION } kind;
    uint32_t                  nfields;
    struct {
        uint32_t              offset;       // field offset in the pointee
        struct lind_arg_spec  spec;         // recursive: a field may itself be ptr/handle/struct
        uint8_t               touched;      // KSplit projection: copy only touched fields
    } fields[];
    uint32_t                  discriminator_arg;  // UNION: which arg/field selects the active arm
};
```

The `z_stream` case is then a `LIND_ABI_PTR` to a `LIND_LAYOUT_STRUCT` whose `next_in`/
`next_out` fields are `LIND_ABI_PTR` buffers (sized from sibling `avail_in`/`avail_out`
fields) and whose `state` field is a `LIND_ABI_HANDLE` (pass-through, never copied) — exactly
the deep-copy-vs-handle split from §6.3.

### 6.5 Layer-1 inference now emits *grate code*, not a config file

We have a major advantage KSplit lacks for closed libraries but *shares* for the libraries
we control: we compile glibc and our test libraries ourselves and have LLVM IR / Wasm.

1. **Build a PDG / use LLVM's existing analyses** over the library's bitcode; represent each
   exported function's parameters as **parameter trees** (PtrSplit) / **projections**
   (KSplit).
2. **Infer direction from per-field READ/WRITE access** (KSplit) — eliminates hand-writing
   `In/Out/InOut`.
3. **Classify pointers with CCured/NesCheck** (safe→singleton, sequential→array,
   wild→ambiguous) to pick category 2 vs 4/5 vs 13. **Infer strings from uses** in string
   functions.
4. **Infer sizes statically**; where impossible, fall back to **PtrSplit-style runtime
   bounds metadata** rather than failing. (We already carry a Wasm linear memory base and
   could thread bounds.)
5. **Emit a coordinator grate automatically** — the `lind_abi_spec` initializers plus the
   `register_lib_handler(...)` calls (and the transitive-copy glue from §6.2) — **plus a
   warning list** for the residue (KSplit's "infer the common case, flag the rest"). KSplit
   already generates IDL + glue + registration; in our model the output is C/Rust grate code,
   which is the concrete bridge from "hand-written `lind_abi_spec`" to "automatic."
6. **Seed where cheap** (Glamdring): a handful of human hints (e.g. "this `void*` is a
   `widget*`", "this handle class is per-cage") collapse most residual ambiguity.

### 6.6 Strategic notes

- **Two registers of automation, by library provenance.** For libraries we build from
  source (glibc, our test libs), pursue *inference* (KSplit/PtrSplit) that auto-emits the
  grate. For opaque/binary libraries (and anything driver-like), the field still uses
  *declarative* per-API stubs (GVirtuS/Cricket) — so the hand-written `lind_abi_spec` path
  is the fallback *and* the inference output format. Same descriptor either way.
- **Decide deep-copy-vs-handle explicitly and early.** It is the one classification whose
  error corrupts memory. When unsure, default to **handle/pass-through + Local execution**
  (safe) and warn — never silently deep-copy an unknown pointer.
- **Projection (touched-fields) is the performance unlock** for the inter-cage `z_stream`
  path: the trusted `safe_copy` should move only accessed fields, not whole structs.
- **A serde-style "copy everything" mode (Sandcrust) is a good correctness baseline** to
  ship first and optimize later with projections — it gets nested structs working
  immediately at a known overhead, and the universal layering (§6.1) means you swap the copy
  primitive without touching the descriptor.
- **RLBox's lesson is cultural**: since handlers are *untrusted*, marking "unverified
  cross-boundary value" a *distinct type* in handler glue turns whole classes of marshalling
  bugs into compile errors before they reach the trusted dispatcher.

### 6.7 Literature-grounded answers to the §13 Open Questions

| # | Open question | Precedent | Recommendation |
|---|---|---|---|
| **1** | Extend `register_handler` or separate `register_lib_handler`? | syscall tables key on a scalar number; library calls need ABI metadata + a (lib,symbol) key. MIDL/syzlang keep the *descriptor* with the interface, not the dispatch core. | **Both, layered:** keep a unified low-level `register_handler(kind,key,handler)` for dispatch; expose `register_lib_handler(...,abi,flags)` as the typed wrapper that owns the ABI descriptor. Don't pollute the syscall path with ABI structs. |
| **2** | How to key library symbols? | syzlang names by (subsystem, call); ELF/`dlsym` resolves by (soname, symbol); GOT-cell identity is the runtime locus. | **Canonical key = hash(soname, symbol)** for registration/lookup stability; **bind to GOT-cell identity at patch time** for the data-plane (so versioned/duplicate symbols and `dlsym` results stay correct — see Q6). Call-IDs are a wire optimization, derived not primary. |
| **3** | ABI metadata with the handler, or separate? | MIDL/syzlang/XDR co-locate the descriptor with the interface; the *trusted marshaller needs it regardless of transport* (§6.1). | **With the handler.** The handler may pick any transport at call time, but the trusted dispatcher must interpret the same `lind_abi_spec` to do `safe_copy`/`translate_handle`. Storing it separately invites desync. |
| **4** | Require pre-start (static) registration first? | Selective-patching systems (and our own §10) prefer install-before-run; KSplit/Glamdring compute everything statically. | **Yes — phase 1 = pre-start patching.** It sidesteps the GOT/thread-safety problems (Q5) and matches the inference flow (the grate is generated and registered before the app runs). Dynamic patching is a later, opt-in mode. |
| **5** | Runtime patching with multiple threads? | LXDs/Nooks quiesce on reconfiguration; dynamic linkers patch GOT atomically; RLBox swaps sandboxes at safe points. | **Atomic single-word GOT/portal-slot swap**, and for live re-routing **quiesce the target cage** (we already have epoch/signal machinery for cross-thread sync). Treat the portal slot as one atomically-publishable pointer; never patch mid-call. |
| **6** | `dlsym`-returned pointers after patching? | The portal *is* the symbol's address once the GOT cell points at it; Cricket/GVirtuS hand back the wrapper, never the real fn. | **`dlsym` must return the portal stub's address**, not the raw function — so indirect calls and `dlsym` agree. This is why the data-plane key binds to GOT-cell identity (Q2): patch the cell, and both call paths follow. |
| **7** | Inheritance across `fork`? | 3i syscall handlers already have fork semantics; our dlopen work replays loader state to children; capabilities are normally inherited. | **Child inherits the parent's library-handler table by default** (copy the per-cage table + handle-table *translations*, not the remote connections). Re-establish transport lazily on first remote call in the child. Mirror exactly how syscall handlers + `LindGOT` already propagate on fork. |
| **8** | Behavior across `pthread_create`? | Threads share the address space and (in Lind) the cage; handler tables are per-cage, not per-thread. | **Threads share the cage's handler table** — no per-thread copy. Only the *handle table* needs thread-safe access (RW-lock / sharded map). Matches the fdtables concurrency model already in the codebase. |
| **9** | Can multiple grates stack/compose for one symbol? | This is core 3i: grates already *stack* at the syscall boundary (clamping/stacking from the early-arch work); Nooks/interposition chains compose. | **Yes — reuse the existing grate-stacking semantics.** A library portal dispatches through the same composition order as syscall grates; each handler may forward to the next or short-circuit (deny/cache). The descriptor is shared down the chain; only the innermost that executes touches real memory. |
| **10** | How does a handler call the original implementation? | Our prototype already uses `call_nested` for the Local fallback; RPC stubs keep a pointer to the real fn; SWIG `action`. | **Expose a `call_original(symbol_key, raw_args)` primitive** that invokes the real (un-patched) function via the saved GOT entry, using `call_nested` to avoid stealing asyncify callbacks. This is the Local branch of the scheduler and the base of the grate stack. |
| **11** | Trusted minimum for inter-process / inter-machine? | Mach kernel only does typed-descriptor transfer + port translation; Cricket/GVirtuS put *all* transport in user space; rCUDA handle-IDs are user-side. | **Trusted minimum = `safe_copy` + `translate_handle` + cage/pointer validation only.** Sockets, RDMA, serialization framing, retries, endpoint selection all live in the **untrusted handler**. The trusted side never opens a network connection. |
| **12** | Should network transport live entirely in user-level handler code? | Same precedents as Q11; this is the RLBox/grate philosophy and the explicit goal of §4.3. | **Yes, entirely.** Because the data *shape* layer (§6.1) is transport-agnostic, the handler can serialize the already-resolved regions/handles and ship them however it likes. Keeps the TCB free of network code — the strongest security argument for this whole design. |

---

## 7. One-paragraph answer to "how much can we learn from them?"

A great deal, and the library-level-3i framing makes the path unusually clear. The
**vocabulary** of argument categories is solved and stable — copy syzlang's type algebra and
MIDL's pointer-kind/`size_is`/`switch_is` attributes to round out `lind_abi_spec` (especially
**opaque handles, transitive struct projections, out-handle sizing, unions, callbacks**).
The **universal inter-cage/process/machine model** is exactly Mach MIG's split: describe the
data *shape* once in `lind_abi_spec`, and let a thin trusted `safe_copy` + `translate_handle`
pair plus an untrusted handler choose the *movement* — which keeps all network/transport code
out of the TCB (the strongest security argument for the design). The **automatic-inference**
layer is an active but mature research line whose best result, **KSplit**, targets a strictly
harder problem than ours (closed-source kernel drivers) and still automates ~98% — its exact
recipe (PDG + projections + CCured pointer classes + READ/WRITE direction inference + fixpoint
traversal + warn-on-residue), built on **PtrSplit's parameter trees and runtime-bounds
fallback**, is directly portable to our LLVM/Wasm toolchain and would emit a coordinator grate
rather than a config file. The **accelerator-remoting** systems hand us the production-tested
pattern for the one category the current design entirely lacks: a **shadow handle table** so
opaque pointers pass through by value without ever being dereferenced across any boundary.
The realistic program is: (1) enrich `lind_abi_spec` to the converged taxonomy (handle +
recursive layout first), (2) layer the marshaller into transport-agnostic shape vs
transport-specific movement, (3) build a KSplit-style analysis to auto-emit the grate with a
warning list, keeping the hand-written descriptor as the fallback for opaque binaries.
