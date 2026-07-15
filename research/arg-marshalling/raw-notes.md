# Raw extraction notes (per source)

Per-source: argument categories handled · declared vs inferred · hard-case handling · limitations.
These are working notes behind `report.md`; quotes are paraphrased from the cited PDFs/docs.

## KSplit (OSDI'22) — `papers/ksplit_osdi22.pdf`  ★ most relevant
- Goal: auto-generate kernel↔driver synchronization (marshalling) code for unmodified drivers. Identifies *shared state* (subset of struct fields touched on both sides) and *how to synchronize* it per boundary crossing.
- Categories = "low-level C idioms" (Fig 2): (a) sized & sentinel arrays, (b) collocated structs (ptr arithmetic), (c) special memory (user vs kernel, device MMIO), (d) recursive data structures, (e) tagged/anonymous unions, (f) opaque `void*` pointers; plus error-pointers (ERR_PTR), container-of.
- **Projections** (from LXDs): IDL lists only the fields the callee reads/writes (parameter-access analysis). `rpc ... ixgbe_xmit_frame(projection sk_buff [alloc(callee)] *skb, projection net_device *netdev)`.
- **Direction (in/out/inout) inferred from per-field READ/WRITE access** map (AM global map).
- **Pointer classification = CCured/NesCheck**: safe (singleton ⇒ deep-copy one), sequential (ptr arithmetic ⇒ array/struct), wild (cast ⇒ ambiguous). Strings inferred from uses in string-manipulation functions.
- **Unions**: reconstruct field names by matching IR access offsets to layout; runtime **discriminator function** picks active arm.
- **Recursive structs**: self-referential projection; generated glue traverses pointers to a **fixpoint with visited-set** (handles cycles).
- **Opaque/error ptr**: resolve `void*` if cast to single type; special-case ERR_PTR.
- **Object lifetime**: hybrid static (find dealloc sites, instrument) + dynamic (runtime tracks objects crossing boundary).
- Philosophy: **infer common case automatically, emit precise warnings for residue.** ixgbe: 2,476 lines IDL generated, 53 manual. 354 drivers similar fraction.
- Built on PDG + parameter trees (PtrSplit), SVF alias analysis, NesCheck. IDL compiler 4,100 LOC C++.
- Limit: multithreaded/concurrency idioms, some wild pointers still need human; relies on source/IR availability.

## PtrSplit (CCS'17) — slides `papers/ptrsplit_ccs17.pdf` + dissertation `papers/SHEN_LIU_Dissertation.pdf`
- First to support **general pointers** in automatic partitioning.
- **PDG** (program dependence graph) + **parameter tree** = structural unfolding of a pointer-typed parameter into transitively reachable components ⇒ automatic deep-copy plan (category 9).
- **Selective pointer-bounds tracking**: lightweight runtime instrumentation carrying each pointer's bounds, so the marshaller knows exact byte count when static size is unknown. Modular (no whole-program pointer analysis).
- Lessons: parameter tree is the right structure for transitive marshalling; fall back to runtime bounds when static inference fails.

## Glamdring (ATC'17) — `papers/glamdring_atc17.pdf`
- Partition an app for SGX from a *few* `sensitive` source/sink annotations; data-flow analysis propagates, computes the partition + generated marshalling stubs at the boundary. "Seed a little, infer the rest."

## Program-mandering (CCS'19) — `papers/program-mandering-ccs2019.pdf`
- Quantitative privilege separation: pick the partition cut that minimizes boundary-crossing / marshalling cost subject to a security objective (Pareto). Relevant to scheduler/cut-choice, not category language.

## RLBox (USENIX Sec'20) — `papers/rlbox_sec20.pdf`
- Not auto-marshalling; **type-system enforcement**. Sandbox-origin data is `tainted<T>`; compiler refuses use until explicit `verify()`/`copy_and_verify()` across boundary. Pointers **swizzled** between app & sandbox address spaces. Callbacks must be **registered**. Turns silent marshalling bugs into compile errors. Used to retrofit isolation around Firefox's libgraphite etc.

## Sandcrust (PLOS'17) — `papers/sandcrust_plos17.pdf`
- One macro annotation; **`serde`-serializes all args + returns** over a pipe to a forked sandbox process. Generic deep/transitive copy "for free"; cost = no sharing, full copy. The "just serialize everything" baseline.

## LXFI (SOSP'11) — `papers/lxfi_sosp11.pdf`
- Kernel-module isolation via **capabilities** + API-integrity annotations stating which references are copied/transferred. Manual precursor to KSplit's automation. Capability = opaque-handle mechanism.

## Cali (AsiaCCS'21) — in `papers/bauer_thesis_cali.pdf`
- Fully-automatic library→separate-process isolation; **PDG at link time** computes shared memory between program and library. User-space sibling of KSplit; reduces shared memory to <0.4%.

## XFI / Wedge / Privtrans — `papers/xfi_osdi06.pdf`, `papers/wedge_nsdi08.pdf`, `papers/privtrans_sec04.pdf`
- Establish the *boundary* (SFI guards; sthreads with tagged memory sharing; programmer-marked split with inserted send/recv). Marshalling largely manual. Foundational, not category-rich.

## Donky / Hodor — `papers/donky_sec20.pdf`, `papers/hodor_atc19.pdf`
- Cheap in-process isolation enforcement (protection keys). About crossing cost, not what to copy.

## Cricket (CC:P&E'22) — `papers/cricket_cpe22.pdf`  ★ opaque-handle lesson
- Intercept CUDA calls → RPC to a server process (separate process per app); server is the same binary loaded as a lib (constructor waits for RPC). **Uses TI-RPC (Sun RPC v2) + rpcgen code generation** for stub creation; rpcbind for discovery; unix or TCP socket.
- **Opaque handles passed by raw value, never dereferenced**: "pass only the raw pointer values, ignoring that they reference a different address space." Real data copied only for `cudaMemcpy(ptr,len)` family. Shared-memory/RDMA optimization for big buffers (needs cudaHostAlloc pinned memory).
- Per-API stubs are hand/codegen — no inference (GPU driver closed).

## GVirtuS (2016) — `papers/gvirtus_arm_2016.pdf`
- Split-driver: developer subclasses **Frontend / Backend / Handler** per function. Communicator serializes a buffer (params + function name) frontend→backend.
- **Shadow pointer map**: device pointers stored in a list; "nature of the pointer" (host vs device) decided by querying the list. Under UVA, managed pointer stored in a map with its size + a malloc'd host shadow; runtime keeps coherence between the two address spaces, copies pinned memory frontend↔backend bound to the valid device pointer.

## rCUDA — `papers/rcuda_aaas_arxiv.pdf` (+ secondary)
- Client/server split of CUDA; client forwards requests. **Assigns identifiers to pointers**; identifier (not data) is packed into the request. Same shadow-handle idea as GVirtuS.

## GPU API remoting characterization (2024) — `papers/2401.13354_gpu_api_remoting_network.pdf`
- Measures cost: serialization/deserialization (S+D) of arguments is a first-order latency term alongside network send. Motivates avoiding pointee copies for handles and using RDMA/shmem for large buffers.

## MIDL / MS RPC (docs)
- `[in]/[out]/[in,out]` direction. `[size_is(n)]` = allocated element count (references another param: `[in] short m; [in,size_is(m)] short a[]`). `[length_is]` = transmitted count (≤ size). `[max_is]` = max index. `[string]` = size from NUL terminator.
- **Multiple pointer levels**: `Proc4 [size_is(,m)] short **pp` (ptr→ptr→m shorts); `Proc6 [size_is(m,n)] short **pp`. **`Proc7 [out, size_is(,*pSize)] my_type **ppMyType`** = callee allocates array whose size is unknown until return, reported via `*pSize` (category 8 / pointer-to-pointer out-param).
- **Pointer kinds**: `ref` (non-null, no alias), `unique` (nullable, no alias), `ptr`/full (may alias / cyclic ⇒ aliasing-aware marshalling). **`[switch_is]/[switch_type]`** = discriminated unions. **Context handles** = server-side opaque handle returned to client.

## syzlang (syzkaller docs)  ★ best ArgSpec template
- `ptr[in|out|inout, T]`, `ptr64`. `buffer[dir]` (ptr to byte array). `array[T]`, `array[T, N:M]`. `string` (zero-term), `stringnoz`.
- **`len[field]`** (element count of another field), **`bytesize[field]`** (byte size) — symbolic, bidirectional size links.
- **`resource fd[int32]`** = opaque handle with special values; tracks producer→consumer dataflow across calls.
- Structs/unions nest; **per-field direction** `(in)/(out)/(inout)`; conditional fields `(if[expr])` for variant layouts. int ranges/flags. Descriptions partly auto-extracted from headers.

## SWIG typemaps (docs)
- Typemap kinds across call lifecycle: `in` (target→C), `check` (validate), `out` (return C→target), **`argout`** (copy out-params back), `freearg` (cleanup).
- Out-param: `%typemap(in, numinputs=0) int *OUTPUT (int tmp){ $1=&tmp; }` + argout returns tmp.
- **Multi-argument typemap**: `%typemap(in)(char *buf,int len){...}` bundles a consecutive (ptr,len) pair into one logical arg. Multi-arg typemaps take precedence over simple ones.
- Pattern matching: exact type+name → exact type → typedef reduction → `SWIGTYPE` default. String typemaps prioritized over generic pointer.

## Mach MIG (documented behavior)
- `.defs` interface → generated client+server stubs. Routine params tagged `in`/`out`/`inout`.
- **Inline vs out-of-line (ool)** data: small data inline in message; large buffers ool, page-mapped copy-on-write (size-driven transport choice).
- Arrays carry explicit counts. **Port rights** are typed message descriptors = capabilities; kernel translates port names between sender/receiver namespaces (opaque-handle translation done in kernel).

## XDR / Sun RPC (RFC 4506)
- Data-description language; `rpcgen` emits paired `xdr_T()` routines used for *both* encode and decode (one (de)serializer per type, composed structurally). Recursive types (linked lists) supported. Basis Cricket reuses for CUDA.
