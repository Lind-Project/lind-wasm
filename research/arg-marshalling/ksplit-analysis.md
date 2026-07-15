# Deep Analysis: KSplit (OSDI 2022) as a Model for Lind Argument Marshalling

*Source: Huang, Narayanan, Detweiler, K. Huang, Tan, Jaeger, Burtsev, "KSplit: Automating Device Driver Isolation," OSDI 2022. Full text in `papers/ksplit_osdi22.pdf`. This note goes deeper than §4.5 of `report.md`.*

---

## 0. TL;DR verdict — does KSplit match what Lind wants?

**Partly, and in the most valuable place — but it is not a drop-in model, and one of Lind's
biggest needs is exactly where KSplit is weakest.** The honest breakdown:

| Dimension | Match? | Detail |
|---|:--:|---|
| **Layer-1 inference** (auto-derive each argument's category/size/direction from code) | ✅ **Strong — best in the literature** | KSplit's entire purpose. Its PDG + parameter-tree + pointer-classification + READ/WRITE-access machinery is exactly what Lind wants to replace hand-written `lind_abi_spec`. |
| **Interposition granularity** (per function-call boundary) | ✅ Strong | KSplit synchronizes state on each cross-domain *call*, like a Lind library portal. |
| **C idiom taxonomy** (the argument categories) | ✅ Strong | KSplit's "ambiguous idioms" *are* Lind's argument categories (arrays, strings, unions, void*, recursive structs, error/opaque pointers, collocated). |
| **"Projection" = copy only touched fields** | ✅ Strong | Directly solves Lind's `z_stream` over-copy problem. |
| **"Infer common case, warn on residue"** | ✅ Strong | Matches Lind's incremental-automation goal; output is an IDL + glue, like Lind's "emit a coordinator grate." |
| **Inter-cage transfer** | ◐ Good | KSplit is same-machine state-sync; maps cleanly to cage↔cage copy. |
| **Inter-machine / inter-process transfer** | ⚠️ **Weak** | KSplit *synchronizes two copies of shared state* assuming **identical type layouts on both sides and a single machine**. It is NOT address-independent serialization. Lind's remote case needs an XDR/protobuf-style serialization layer *underneath* KSplit's inference. |
| **Opaque handles** (pass-by-token, never deref) | ⚠️ **Weak — and this is Lind's #1 need** | KSplit barely needs handle translation because isolated domains share a synchronized view of the *same* objects. Its answer to `void*` is "resolve the cast or warn," not "treat as an opaque token." Lind's shadow-handle-table comes from the **accelerator-remoting** cluster (rCUDA/GVirtuS/Cricket), **not** KSplit. |
| **Concurrency primitives** (shared spinlock/RCU/atomics across the boundary) | ❌ Irrelevant | A whole KSplit subsystem (§4.4) that Lind library calls don't need. |
| **Threat model** | ⚠️ Weaker than Lind may want | KSplit **trusts the kernel** and isolates a *buggy-but-not-malicious* driver; it explicitly defers malicious-driver defense to future work. Lind often wants mutual distrust / malicious-library defense. |

**Bottom line for Lind:** adopt KSplit's **inference engine and IDL/projection model** (layer 1)
nearly wholesale for the *inter-cage* case and for generating `lind_abi_spec`; **layer an
address-independent serialization step under it** for inter-process/inter-machine; **add a
first-class opaque-handle/shadow-table category from the remoting cluster** (KSplit won't give
you this); and **drop the concurrency-primitive machinery**. KSplit is the right *layer-1*
model, not the right *transport* model.

---

## 1. The KSplit pipeline (one picture)

```
 kernel + driver source
        │  clang
        ▼
   LLVM IR (compiled at -O0 to preserve source semantics)
        │
        ▼
   PDG construction  ── interprocedural Program Dependence Graph
        │             (SVF intra-procedural alias analysis,
        │              glued inter-procedurally via parameter trees)
        ▼
   ┌──────────────────────────────────────────────┐
   │ (1) Shared vs private data analysis  (§4.2)    │  → which struct fields cross the boundary
   │ (2) Parameter-access analysis        (§4.3)    │  → READ/WRITE per field ⇒ in/out/inout
   │ (3) Concurrency-primitive analysis   (§4.4)    │  → [Lind: not needed]
   │ (4) Idiom / pointer classification   (§5)      │  → array? string? union? void*? recursive?
   └──────────────────────────────────────────────┘
        │  emits …
        ▼
   KSplit IDL  (projections + attributes)  +  WARNINGS for unresolved residue
        │  IDL compiler (4,100 LOC C++)
        ▼
   generated glue code  (marshals/syncs on each boundary crossing)
        +  runtime object-lifetime tracking
```

The key conceptual output is a **projection** per composite argument: a description listing
*only the fields that actually cross the boundary*, each annotated with direction and
(for ambiguous fields) a marshalling attribute. The IDL compiler turns projections into glue.

---

## 2. The two foundations Lind would reuse

### 2.1 PDG + parameter trees (the structural backbone)

KSplit represents the program as an **interprocedural Program Dependence Graph** (nodes =
LLVM instructions; edges = control/data dependence). The scalability trick is the
**parameter tree** (borrowed from PtrSplit): for each function argument it builds a tree of
the storage locations the callee can reach — root = the pointer, children = the pointee's
fields, recursively. Example for `msr_read(struct file *file, int count)`: a tree rooted at
`file:struct file*` with child `*file:struct file`, child `f_inode:struct inode*`, etc.,
each node field-sensitive.

Alias analysis is done **modularly**: SVF computes aliases *intra*-procedurally per function;
parameter trees glue actual↔formal arguments at call sites to propagate *inter*-procedurally
(context-insensitive). This is what lets it scale to the kernel (the ixgbe analysis touches
5,782 functions). **Lind relevance:** the parameter tree *is* the recursive `lind_layout`
proposed in `report.md` §6.4 — the data structure that drives transitive copy of `z_stream`.

### 2.2 Shared vs private state (the projection optimizer)

A 4-step, type-scoped, field-sensitive algorithm decides which fields actually cross:
1. collect struct types reachable transitively through interface params, globals, and
   interrupt handlers ("shared struct types"); (does **not** use the CFG, so it catches
   interrupt handlers that have no caller)
2. find all functions touching those types;
3. use the PDG to collect *field accesses* per type;
4. a field accessed by **both** sides ⇒ **shared**; otherwise **private**.

The payoff is dramatic and is the single most transferable performance idea: for ixgbe,
**999,136 fields are transitively reachable, only 4,509 are accessed, 3,029 are shared, and a
nested-crossing optimization further cuts this to 2,669.** Naïve "deep copy" would move
3 orders of magnitude more data. **Lind relevance:** this is exactly "don't copy the whole
`z_stream`, copy the touched fields" — the projection is the `touched` bitmap in §6.4.

> ⚠️ Assumption to inherit carefully: shared-state detection is **type-based** — it assumes
> if one instance of a type shares a field, all instances do. This over-approximates (extra
> copying, harmless) and can under-approximate if state is shared *without* using the shared
> type (a correctness risk; the authors found none in practice). It is also field-instance
> *insensitive*.

---

## 3. Direction inference — the gem for Lind (`ptr_direction` for free)

This is the part Lind most wants and rarely appears elsewhere. KSplit's **parameter-access
analysis** (Algorithm 1, worklist to a fixpoint) computes, for every parameter-tree node, an
**access label** ∈ {READ, WRITE} describing how the *callee* uses that storage:

- fields with **READ** ⇒ copied **caller→callee on the call** (an *in* field);
- fields with **WRITE** ⇒ copied **callee→caller on return** (an *out* field);
- both ⇒ *inout*.

So `lind_arg_spec.ptr_direction` (IN/OUT/INOUT) — which Lind hand-writes today — is
**inferred mechanically from read/write access**, no human needed. A refinement removes
fields that are only touched in the *caller's* domain during nested boundary crossings, so it
doesn't over-send (their example: when `k` calls `d`, send only the field `d` actually reads).

**Lind caveat:** this analysis runs on the *callee* (driver) and the functions reachable from
it — which for Lind is the **library** side (good, we have glibc/zlib source). But the
nested-crossing refinement and some inference lean on seeing the *caller* too; Lind's callers
are arbitrary apps, so expect slightly lower precision than KSplit reports.

---

## 4. How KSplit handles each argument-marshalling scenario  ★ (the core of your ask)

For every scenario: *what it is · how KSplit decides it · the IDL attribute · auto or
manual/warning · the Lind mapping.* Classification leans on **CCured/NesCheck** pointer
classes: **safe** (single object ⇒ singleton), **sequential** (used in pointer arithmetic ⇒
array/struct), **wild** (involved in casts ⇒ ambiguous).

### 4.1 Scalars / by-value
- **Decide:** non-pointer LLVM type. **IDL:** plain field. **Auto:** always.
- **Overhead (Table 4):** an 8-byte integer ≈ 532 cycles round-trip.
- **Lind:** `LIND_ABI_I32/I64/F32/F64` — already covered.

### 4.2 Singleton pointer (fixed pointee) — CCured *safe*
- **Decide:** pointer never used in arithmetic ⇒ references exactly one object ⇒ deep-copy
  `sizeof(*p)`. **IDL:** a `projection` of the pointee type. **Auto:** yes (the common case —
  e.g. for ixgbe, 143 of 144 void-pointers and the vast majority of typed pointers are auto).
- **Lind:** `LIND_ABI_PTR` + `LIND_SIZE_CONST` / pointee `lind_layout`.

### 4.3 Sized array (size known at allocation)
- **Decide:** sequential pointer whose allocation size is statically recoverable.
- **IDL:** `[alloc_sized<callee>(self->field)]` — *callee allocates, size = another field.*
- **Auto:** when the size source is found; **else a WARNING** ("array of undetermined size").
  For ixgbe, **27 of 119 array pointers needed manual size attributes.**
- **Lind:** `LIND_SIZE_FROM_ARG_VALUE` / `FROM_ARG_POINTEE` — already covered for the flat case.

### 4.4 String (NUL-terminated)
- **Decide:** a `char*` whose **aliases flow into string functions** (`strcmp`, `strcpy`, …)
  ⇒ classified as a string (size from the terminator). **IDL:** string attribute.
- **Auto:** when a string-function use is visible; **else WARNING / misclassification risk**
  (strings *not* passed to string fns get mistaken for singletons — see §6).
- **Overhead:** a 256-byte string ≈ 1310 cycles — *more than a 4 KB void buffer (919)* because
  it must be scanned for the terminator. **Lind:** `LIND_SIZE_CSTR` — covered.

### 4.5 Sentinel-terminated array
- **Decide:** array ended by a zero/`{}` element (e.g. `pci_device_id[]`, `argv`).
- **IDL:** sentinel-size attribute. **Auto:** sometimes; **else WARNING.**
- **Lind:** proposed `LIND_SIZE_SENTINEL` (not in current `lind_abi_spec`).

### 4.6 In / out / inout direction
- **Decide:** READ/WRITE access labels from §3. **Auto:** yes — the standout feature.
- **Lind:** auto-fills `ptr_direction`.

### 4.7 Nested / recursive / cyclic data structures
- **Decide:** parameter tree + shared-state analysis produce a projection that **points to a
  projection of its own type** for self-referential fields (linked lists, trees, graphs).
- **IDL:** nested `projection T *field;`. **Glue:** **traverses pointers to a fixpoint with a
  visited-set**, so cycles terminate. **Auto:** yes for the traversal structure.
- **Example:** `sk_buff` carries an optional fragment list; KSplit marshals the whole chain.
- **Lind:** recursive `lind_layout` + visited-set in the generated glue — the correct
  `z_stream`/transitive-copy mechanism.

### 4.8 Tagged / anonymous unions
- **Decide:** union field names are lost in LLVM IR (compiler treats unions as raw bytes), so
  KSplit **reconstructs field identity by matching IR access offsets to the struct layout.**
- **IDL:** union projection. **Resolve active arm:** requires a **user-supplied discriminator
  function** (≈ MIDL `switch_is`). **Auto:** layout reconstruction yes; **active-arm selection
  is manual / a WARNING** — anonymous unions are the *dominant* warning source (can-raw: 30
  anon unions ⇒ 35 warnings; alx: 17 anon unions ⇒ 22 warnings).
- **Lind:** proposed `LIND_LAYOUT_UNION` + `discriminator_arg` — and expect this to need human
  help, as KSplit does.

### 4.9 Opaque / `void*` polymorphic pointers — CCured *wild*
- **Decide:** pointer involved in type casts. KSplit resolves it **if it's cast to a single
  concrete type** at its uses; otherwise emits a WARNING. **Auto:** mostly (ixgbe: 143 wild
  void pointers, only **1** needed manual work; 3 non-void cast pointers inspected).
- ⚠️ **Critical for Lind:** KSplit's notion of "resolve `void*`" means *find the real type and
  deep-copy it* — because in kernel isolation both domains see a synchronized copy of the same
  object. It does **not** mean "treat as an opaque token that must never be dereferenced."
  **Lind's opaque-handle need (a `FILE*`/fd/GPU-handle/remote pointer that must pass through by
  value) is a different mechanism KSplit does not provide** — that's the shadow handle table
  from rCUDA/GVirtuS/Cricket. KSplit here is *not* the model.

### 4.10 Error pointers (`ERR_PTR`)
- **Decide:** kernel idiom where a function returns either a valid object or a specially-formed
  error pointer. KSplit detects the pattern. **Auto:** yes (ixgbe: 1 such function, auto-handled).
- **Lind:** minor — relevant for glibc functions returning sentinel pointers; a flag on the
  return spec.

### 4.11 Special memory (user memory, ioremap/DMA/device MMIO)
- **Decide:** detects pointers into user space (`__user`), `ioremap`'d device memory, and DMA
  regions, which need region-specific handling (don't treat device MMIO as normal RAM).
- **IDL:** region attributes. **Auto:** identifies the regions (ixgbe: 5 user/ioremap regions
  found); handling may be special-cased. **Auto:** detection yes.
- **Lind:** analogous to distinguishing Wasm-linear-memory pointers from host pointers; relevant
  to the trusted `safe_copy` knowing *which* address space a region lives in.

### 4.12 Collocated structures / container-of / pointer arithmetic — the `within` attribute
- **Decide:** kernel collocates objects in one allocation and navigates with arithmetic (e.g.
  `skb_shinfo` lives *inside* the `sk_buff` data buffer; `tail`/`end` are offsets into `data`).
  The low-level PDG reveals the object is allocated within another.
- **IDL:** `[within<self->head, self->true_size>]` — *this field is a pointer/offset that must
  lie within a given base+size range.* **Auto:** detects the pattern, **but the range bound is
  specified MANUALLY** (Listing 1). Container-of is also the largest "wild (other)" warning bucket.
- **Lind:** rare in clean library ABIs but exactly the kind of idiom that should become a
  WARNING + force-Local rather than a silent copy.

### 4.13 Object lifetime / ownership (alloc & free across the boundary)
- **Decide:** `[alloc(callee)]`/`[alloc(caller)]` attributes mark which side allocates; a
  **hybrid static+dynamic** scheme tracks objects: the runtime registers each new object
  crossing the boundary, while static analysis finds **deallocation sites** and instruments
  them to propagate frees. **Auto:** mostly.
- **Lind:** maps to the trusted handle table's allocate/free of shadow objects; important for
  out-handles (`cudaMalloc`-shaped) and for not leaking remote allocations.

### 4.14 Concurrency primitives (atomics, spinlocks, seqlocks, RCU, mutexes) — *Lind: skip*
- **Decide:** finds lock/unlock pairs in the CFG (same lock via alias analysis); syncs shared
  state *right after lock acquire* (READ) and *right before release* (WRITE); atomics update a
  **single primary copy in the kernel** (drivers call in). **Auto:** yes.
- **Lind:** **not applicable** — a library call doesn't hold a lock shared with the caller
  across the interposition boundary. This entire subsystem is dead weight for Lind. (Encouraging
  data point: for ixgbe, 70/73 critical sections, all RCU, and all seqlocks turned out
  *private* — i.e., concurrency rarely crosses the boundary even in a kernel.)

---

## 5. The IDL: projections + attribute vocabulary

The real `sk_buff` projection (Listing 1 in the paper), annotated:

```c
projection<struct sk_buff> skb_xmit {        // name a field-subset view of sk_buff
    projection net_device *dev;              // 4.7 nested pointer → another projection
    unsigned int len;                        // 4.1 scalar, shared
    unsigned int data_len;
    ...
    void * [alloc_sized<callee>(self->true_size)] head;  // 4.3 callee allocates, size=field
    void * [within<self->head, self->true_size>] data;   // 4.12 ptr inside head's range
    unsigned int [within<_, self->true_size>] tail;       // 4.12 offset within range
    unsigned int [within<_, self->true_size>] end;
};
```

Attribute vocabulary observed: `projection<T>`, nested `projection T *`, `[alloc(callee)]` /
`[alloc(caller)]`, `[alloc_sized<callee>(expr)]`, `[within<base, size>]`, plus
string/size/in/out/user-memory attributes and a user-supplied union discriminator. **This maps
almost field-for-field onto the proposed `lind_abi_spec` v2** (`report.md` §6.4): projection ⇒
`lind_layout` with a `touched` set; `alloc_sized` ⇒ `LIND_SIZE_FROM_ARG_*` + alloc flag;
nested projection ⇒ recursive `pointee`; `within` ⇒ a bounds-checked variant; discriminator ⇒
`discriminator_arg`.

---

## 6. How automatic is it *really*? (the numbers that matter)

For the flagship **ixgbe** driver (27K SLOC, 2,000+ functions):

| Metric | Value |
|---|---|
| Fields transitively reachable across the boundary | 999,136 |
| …actually accessed | 4,509 |
| …**shared** (need marshalling) | 3,029 → **2,669** after nested-crossing opt |
| Pointers requiring marshalling | 1,529 |
| …**needing manual inspection** | **31** (1 void-wild + 3 cast-wild + 27 array sizes) |
| …**silently misclassified** | **7** |
| Generated IDL | 2,476 lines |
| …**manually changed** | 53 lines (**2%**) |
| Driver code changes | 19 lines (mostly macro→function) |
| Analysis time | 190–546 s (complex); <60 s (simple) |

Generality: applied to **354 drivers** across 9 subsystems; per-driver "problematic" (non-
singleton) pointer counts stay low, so the authors project similar ~98% automation broadly.
Cross-driver reuse is high (alx vs ixgbe: shares 73 functions; needed 6 annotation + 41 IDL
line changes).

**The 7 misclassifications are the cautionary tale for Lind.** All were *sequential pointers
wrongly called singletons* because **CCured couldn't see the pointer arithmetic** — it happened
in **uninstrumented library code** (a string passed to a string lib; a DMA region; 4 pointers
passed to `memcpy()`). Root cause: *not analyzing the called library.* For Lind this is a
direct warning: **if you don't analyze the transitive callee (or the app), you will
under-classify, and under-classification of a buffer-as-singleton truncates the copy.** Mitigation
the authors suggest (and Lind should adopt): treat "pointer passed to a known library function
of category X" as a classification hint (e.g. arg to `strcmp` ⇒ sequential/string).

Marshalling cost (Table 4, round-trip cycles): Null 502 · Integer(8B) 532 · Array(256B) 690 ·
String(256B) 1310 · Void(4KB) 919 · Union(24+32B) 710. Takeaway: *the copy is cheap; the
category-specific scanning (strings) and discrimination (unions) dominate.* End-to-end, an
isolated ixgbe stays within 5.4–18.7% of native on memcached.

---

## 7. Limitations / assumptions Lind must keep in mind

1. **Single-machine state synchronization, not serialization.** KSplit keeps two copies of
   shared state in sync and assumes both domains compile the *same struct layouts*. It never
   confronts endianness, pointer-width, or padding differences. **Lind inter-machine needs an
   XDR/protobuf-style canonical encoding layered beneath KSplit-style inference.**
2. **Needs LLVM IR for both sides** (kernel + driver), compiled at `-O0`. Lind has the library
   (glibc/zlib) but not arbitrary app callers; expect reduced precision and the §6
   library-blindness misclassification risk.
3. **No first-class opaque-handle mechanism** — its model assumes shared object identity across
   the boundary. Lind's #1 category (pass-by-token handle) must come from elsewhere.
4. **Type-based shared-state assumption** can under-approximate (rare, correctness risk) and is
   field-instance-insensitive (over-approximates).
5. **Manual residue persists:** array sizes, `within` ranges, and union discriminators need
   humans. The realistic bar is ~98% auto, not 100%.
6. **Weaker threat model:** trusts one domain (kernel), isolates a *non-malicious* buggy driver.
   Resource exhaustion, protocol violations, use-after-free from the isolated side, and
   malicious drivers are explicitly **out of scope**. Lind's mutual-distrust goals exceed this.

---

## 8. Concrete recommendation for Lind

**Take (port nearly as-is into the layer-1 analysis that emits `lind_abi_spec`/grate code):**
- PDG + **parameter trees** as the recursive layout backbone (shared with PtrSplit).
- **Shared-vs-private / projection** analysis → the `touched` field set (the `z_stream` win).
- **Parameter-access READ/WRITE → direction inference** → auto-fill `ptr_direction`.
- **CCured/NesCheck pointer classes** (safe/sequential/wild) → singleton vs array vs ambiguous.
- **String-from-use** and **library-function-as-hint** classification (and *do* analyze callees
  to avoid the §6 misclassification trap).
- **Recursive projection + fixpoint/visited-set traversal** → transitive deep copy.
- **Union offset-reconstruction + discriminator function**, and the **`within`/`alloc_sized`**
  attributes, plus **warn-on-residue** as the operating philosophy.
- **Hybrid static+dynamic object-lifetime tracking** for the handle/shadow table.

**Adapt / add (KSplit does not give these):**
- An **address-independent serialization layer** (XDR/protobuf-style) beneath the inference, so
  one inferred descriptor serves inter-cage *and* inter-process *and* inter-machine — the
  universal-model goal (see `report.md` §6.1).
- A **first-class opaque-handle category + shadow handle table** (rCUDA/GVirtuS/Cricket), since
  cages/machines don't share address spaces. *This is the single most important thing KSplit
  won't teach you.*
- A **stronger trust posture** (mutual distrust) if that's Lind's goal — KSplit assumes a
  trusted side.

**Drop:**
- The entire **concurrency-primitive synchronization** subsystem (§4.4) — not applicable to
  library-call interposition.

**Net:** KSplit is the right answer to "how do we *infer* the marshalling rules automatically"
— which is precisely Lind's open layer-1 problem — and its IDL/projection design is a near-exact
template for an inferred `lind_abi_spec`. It is *not* the answer to "how do we move bytes across
a cage/process/machine boundary" (serialization) or "how do we pass things we must not copy"
(handles); for those, combine it with the IDL/RPC and accelerator-remoting clusters from the
main survey.
