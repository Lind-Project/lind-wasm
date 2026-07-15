# KSplit pdg — Source-Level Inference Analysis

*Primary source: `https://github.com/ARISTODE/program-dependence-graph` branch `dev_ksplit`, cloned at `/home/lind/inference-refs/pdg`. SVF submodule not cloned (alias internals out of scope). All `file:line` citations below are into that tree. This document supersedes/corrects `ksplit-analysis.md`, which is a paper-level secondary source.*

This note documents **how the KSplit static analyses actually compute each piece of marshalling information**, and maps it onto our `lind_marshal.h` descriptor types (`/home/lind/lind-wasm/tests/grate-tests/lib-interpose/lind_marshal.h`).

---

## 0. Big-picture correction vs. the paper notes

The prior note (`ksplit-analysis.md`) is mostly accurate at the algorithmic level, but the **source reveals it is a research prototype, not a clean library**:

- It is **kernel/driver-specific and stats-driven**. The whole thing is bootstrapped off a pile of plaintext side files written by an earlier pass (`driver_funcs`, `imported_funcs`, `exported_funcs`, `sentinel_fields`, `shared_struct_types`, `driver_globalvar_names`, `driver_exported_func_symbols`, `global_op_struct_names`). See `BoundaryAnalysis::dumpToFiles` (`src/BoundaryAnalysis.cpp:182`) which writes them and every later pass's `readFuncsFromFile`/`read*` that consumes them.
- The IDL it emits is a **kernel-RPC IDL** ("LCD IDL": `rpc`, `rpc_ptr`, `rpc_export`, `projection`, `[in]/[out]`, `[alloc(caller/callee)]`, `[bind]`, `array<T,size>`, `string`). It is *not* a binary descriptor like ours. We map its **decisions** onto our struct fields, not its text.
- **KSplit infers a buffer length only from *visible dataflow*, never from a heuristic over sibling arguments.** Two code-visible mechanisms exist: (a) **allocation size** (`ObjectSizeOffsetEvaluator` + `malloc`/`kmalloc` arg, `NesCheck.cpp:193`) — used for pointer classification + runtime bounds; and (b) the **literal copy length** read from `copy_{from,to}_user`'s 3rd operand (`inferUserAnnotation`, `DataAccessAnalysis.cpp:1270`), constant-folded to `user<{{N}}>` or else `user<size_unknown>`; plus a *disabled* sibling-containment annotation (`inferMayWithin`, call site commented at `:672`). When none is visible, a Seq pointer becomes `array<T, size_unknown>` (`:703`) for a human to fill in. What it does **not** do vs. our `LIND_SIZE_FROM_ARG`: it never *guesses* a length from a sibling integer arg's type/position, and it does not encode a runtime-variable "length = argument N" dependency in the IDL (it emits a constant, or `size_unknown`, and leans on runtime bounds tracking for runtime-variable sizes). My earlier "no (buf,len) pairing" phrasing was wrong — the accurate statement is "dataflow-only length recovery, no heuristic sibling-arg pairing."
- **There is no opaque-handle concept at all.** Confirmed: every pointer is either copied (singleton/seq), flagged (wild), or made a `projection`. No pass-by-token. (Our `LIND_ARG_HANDLE` / `LIND_RET_HANDLE` has no KSplit analog.)

---

## 1. Pipeline & pass order

Each analysis is a separate LLVM `ModulePass` registered with `RegisterPass`. The README mandates this order; later passes consume files written by earlier ones.

| # | Pass flag | Class / registration | File | Produces |
|---|---|---|---|---|
| 0 | `-pdg` | `ProgramDependencyGraph` | `src/ProgramDependencyGraph.cpp` | the interprocedural PDG + per-arg **parameter trees** |
| 1 | `-output-boundary-info` | `BoundaryAnalysis` (`src/BoundaryAnalysis.cpp:207`) | `BoundaryAnalysis.cpp` | the boundary func set → plaintext files |
| 2 | `-shared-fields` | `SharedFieldsAnalysis` (`src/SharedFieldsAnalysis.cpp:275`) | `SharedFieldsAnalysis.cpp` | type-scoped shared field-id set (intersection) |
| 3 | `-shared-data` | `SharedDataAnalysis` (`src/SharedDataAnalysis.cpp:633`) | `SharedDataAnalysis.cpp` | shared struct types, shared-field-ids, string-field-ids, per-type **type trees** |
| 4 | `-atomic-region` | `AtomicRegionAnalysis` | `src/AtomicRegionAnalysis.cpp` | concurrency regions (**Lind: skip**) |
| 5 | `-nescheck` | `NesCheck::NesCheckPass` | `src/NesCheck.cpp` | CCured pointer classes (safe/seq/wild) per LLVM value |
| 6 | `-daa` | `DataAccessAnalysis` (`src/DataAccessAnalysis.cpp:1656`) | `DataAccessAnalysis.cpp` | **read/write access tags → direction**, and **emits the IDL** |

Entry points for IDL generation are `DataAccessAnalysis::generateSyncStubsForBoundaryFunctions` (`src/DataAccessAnalysis.cpp:71`) and `generateSyncStubsForGlobalVars` (`:116`); both write `kernel.idl`. `runOnModule` (`:37`) first calls `computeDataAccessForFuncArgs` for every defined function to populate access tags before any IDL text is produced.

`-daa` `getAnalysisUsage` requires `SharedDataAnalysis` (`src/DataAccessAnalysis.cpp:21`), which requires `ProgramDependencyGraph` (`src/SharedDataAnalysis.cpp:7`). NesCheck is run separately and its result is read by NesCheck-internal `classifyBoundaryPtrs` which writes `setSeqPtr()` onto tree nodes (`src/NesCheck.cpp:700`).

---

## 2. Per-analysis breakdown

### 2.1 Parameter trees (`Tree.cpp` / `Tree.hh`) → our `lind_layout`

**Plain English.** For each function argument (and the return value) KSplit builds a `Tree`: root = the formal argument; children = the pointee, then each struct field, recursively, **driven entirely by DWARF debug info (`DIType`)**, not by LLVM struct types. This is the structural skeleton onto which all access/sharing info is hung.

**Construction.**
- Root nodes are built per arg in `FunctionWrapper::buildFormalTreeForArgs` (`src/FunctionWrapper.cpp:37`): it grabs the arg's `DILocalVariable` from the `llvm.dbg.declare`, makes a `FORMAL_IN` root `TreeNode` from `di_local_var->getType()` (`:49`), and stores the `DILocalVariable` (`:50`, used later to recover the source-level parameter name). Return value tree: `buildFormalTreesForRetVal` (`:75`).
- `Tree::build(max_tree_depth=6)` (`src/Tree.cpp:156`) is a **BFS expansion capped at depth 6** (the shared-type trees use depth 2: `SharedDataAnalysis.cpp:298`). **Cycle handling = the depth cap only**; there is no visited-set. Self-referential structs (linked lists) terminate because depth 6 is hit, *and* are special-cased at IDL time (see 2.6). So the paper's "fixpoint with visited set" is **runtime glue, not present in this analysis code** — here it's just a depth bound.
- `TreeNode::expandNode` (`src/Tree.cpp:31`) is the unfold step:
  - strips member/typedef tags (`stripMemberTag`, `stripAttributes`, `:36-37`);
  - if pointer → one child = the pointee (`getLowestDIType`, `:45`), edge `PARAMETER_FIELD`;
  - if "projectable" (struct/union composite) → one child **per DWARF field element** (`getElements()`, `:55-63`), each child a `PARAMETER_FIELD`.
- **Field offset/size:** the tree node does **not** store a numeric byte offset. Field identity is the `DIType` itself; offset matching against IR is done on demand via `pdgutils::isGEPOffsetMatchDIOffset(DIType, GEP)` (used in `computeDerivedAddrVarsFromParent`, `src/Tree.cpp:106`, and in `SharedFieldsAnalysis.cpp:136`). Sizes come from DWARF (`getSizeInBits`, used for bitfields at `DataAccessAnalysis.cpp:665`). **This is the key wasm32 gap: KSplit keeps DWARF/native offsets; we need wasm32 re-layout (see §5).**
- **Address-variable binding** (`computeDerivedAddrVarsFromParent`, `src/Tree.cpp:70`) is how a tree node learns *which LLVM SSA values touch it*: it walks the parent's "addr vars," and for each `GetElementPtrInst` whose offset matches this field's DWARF offset (`:106`), adds the GEP as an addr-var of the child. Loads also propagate (`:98`). These addr-vars are what the access analysis later inspects for read/write.

**Fills:** `struct lind_layout` (kind/nfields/fields), `struct lind_field.offset` (conceptually — KSplit defers it to DWARF), and the recursion that drives `_lind_pre_ptr`'s field walk. `TreeNode.is_string/is_sentinel/_is_seq_ptr/is_shared` annotations (`Tree.hh:54-60`) become per-field metadata.

### 2.2 Direction inference (`DataAccessAnalysis.cpp`) → `lind_ptr_direction`

**Plain English.** For every tree node, collect READ/WRITE access tags by looking at how the node's addr-vars (and inter-procedurally reachable values) are used. READ⇒`[in]`, WRITE⇒`[out]`, both⇒`[in,out]`.

**Key functions / IR keys.**
- `computeDataAccessTagsForVal` (`src/DataAccessAnalysis.cpp:341`) → `pdgutils::hasReadAccess` / `hasWriteAccess`.
  - **READ** = the value is the pointer operand of a `LoadInst`, or the base of a `GetElementPtrInst` (`PDGUtils.cpp:142`). I.e. "something was read out of this storage."
  - **WRITE** = the value is the pointer operand of a `StoreInst` whose stored value is not an `Argument`, **or** it's passed to a known data-writing libcall (`dataWriteLibFuncs`) / inline-asm with a write constraint (`PDGUtils.cpp:157`).
- `computeDataAccessForTreeNode` (`:351`) runs this over all addr-vars (`:450-465`), then does **inter-procedural** propagation: it follows `PARAMETER_IN`/`DATA_RET` edges to callee formal nodes (`findNodesReachedByEdges`, `:477`) and unions their access tags in (`:498-502`). A **domain filter** (`:493`) drops accesses that happen in the *same* domain as the boundary function (the "nested-crossing" refinement — only count accesses on the *other* side).
- Tags are turned into annotation strings in `inferTreeNodeAnnotations` (`:1336`): READ→`"in"`, WRITE→`"out"` (`:1387-1390`). For the return value, `"in"` is stripped (`:680`, a return can't be copied *in*).
- `setCanOptOut(true)` (`:518`) marks a node whose only accesses are cross-domain and not intra — used to **drop fields entirely** from the projection (`:656`). This is the projection optimizer working with the shared-field filter.

**Crucial subtlety:** a field with **no** access tags is omitted from the projection entirely (`generateIDLFromTreeNode`, `:632`: `if (getAccessTags().size()==0 && !is_func_ptr) continue;`). So "touched" and "direction" are computed by the *same* mechanism — absence of any tag ⇒ field not marshalled.

**Fills:** `lind_arg_spec.ptr_direction` (IN/OUT/INOUT) and, by tag-absence, the `touched` bit.

### 2.3 Shared-vs-private fields → our `touched` / projection bit

There are **two** layers here, in two files.

**(a) `SharedFieldsAnalysis.cpp` — coarse type-scoped intersection.**
- `propagateDebuggingInfoInFunc` (`:51`) binds a `DIType` to every load/store/GEP/cast value (`computeInstDIType`, `:75`) so a field access can be named.
- `computeAccessedFieldsInFunc` (`:224`): for each load/store/GEP, compute a `(parent_type, field)` string id via `pdgutils::computeFieldID` (`PDGUtils.cpp:372` = `trim(parentTypeName + fieldName)`), and bucket it into `_driver_access_fields` or `_kernel_access_fields` depending on whether the enclosing function is a driver func (`computeAccessFields`, `:183`).
- `computeSharedAccessFields` (`:260`): **`std::set_intersection` of the two sets** → `_shared_fields`. *That is literally the "accessed on both sides" rule.* A field id present in both driver and kernel access sets is shared.
- It also emits **wild-cast warnings** here (`printWarningsForUnsafeTypeCastsOnInst`, `:197`): a `struct* → struct*` bitcast on a value that has debug info ⇒ "potential wild casting that may cause missing shared fields" + `_num_wild_cast++` (`:216-217`).

**(b) `SharedDataAnalysis.cpp` — field-instance-precise, tree-based.**
- `computeSharedStructDITypes` (`:158`): a struct type is **shared** iff its name appears in *both* a driver-function instruction and a kernel-function instruction. This is again a type-name intersection, not instance-sensitive — confirming the paper's "type-based, instance-insensitive" caveat.
- `buildTreesForSharedStructDIType` (`:281`): for each shared struct type, build a depth-2 type-tree, gather **all module variables of that type** (`computeVarsWithDITypeInModule`, `:347`) as addr-vars, and connect them.
- `computeSharedFieldID` (`:436`): walk each type tree; a struct-field node is added to `_shared_field_id` if **`isTreeNodeShared`** (`:370`) — which checks the node's addr-vars and returns true iff at least one addr-var lives in a driver func **and** at least one in a kernel func (`:382-395`). Function-pointer fields are always treated as shared (`:457`). Two hardcoded ids are force-added (`:483-484`, `inode` rdev/devt — a prototype hack).
- The shared-field filter is applied at IDL time: `generateIDLFromTreeNode` skips a field when `SharedDataFlag && !isSharedFieldID(field_id) && ...` (`DataAccessAnalysis.cpp:646`); and an entire non-shared struct subtree is pruned (`generateIDLFromArgTree`, `:947`).

**Fills:** `struct lind_field.touched` — a field is marshalled (touched=1) iff it is **both accessed (has a tag) and shared (accessed on both sides)**, unless overridden (func-ptr, global-op-struct, sentinel).

### 2.4 Pointer classification (`NesCheck.cpp` + `AnalysisState.cpp`) → SCALAR / array / wild

**Plain English.** A CCured-style monotone lattice over pointer SSA values: `Safe(0) < Seq(1) < Dyn(2)`. Start every pointer Safe; raise it as evidence accrues; the lattice only goes up (`ClassifyPointerVariable`, `src/AnalysisState.cpp:25-39`: `if (classification < ptrType) classification = ptrType;`).

**The three classes and how IR decides them (`processInstruction`, `src/NesCheck.cpp:851`):**
- **Safe (singleton).** Default at registration (`AnalysisState.cpp:20`). A pointer only ever loaded/stored/`GEP`'d at offset 0. ⇒ references exactly one object ⇒ deep-copy `sizeof(*p)`. Maps to our `LIND_ARG_PTR` + `LIND_SIZE_CONST` (or a `lind_layout`).
- **Seq (array / pointer-arithmetic).** Raised to `Seq` when a `GetElementPtrInst` has **non-zero indices into a non-struct element** (`:1114-1126`: non-struct → `Seq`, struct → `Safe`). I.e. genuine pointer arithmetic over an element array. Maps to a sized/unbounded array. At `NesCheck.cpp:700`, boundary seq-pointers get `front->setSeqPtr()`, and the IDL emits `array<T, size_unknown>` (`DataAccessAnalysis.cpp:692-705`).
- **Dyn / "wild" (ambiguous).** Raised to `Dyn` on **type-incompatible casts**: a `CastInst` where the source/dest pointer-indirection count differs, or integer-ness of the innermost type differs (`:1166-1192`). This is the `void*`/reinterpret case. Wild non-void pointers are exactly what KSplit flags for human review (`ksplitRecordPtrType`, `:746-757` prints "Find non-void wild ptr").

**Size tracking** rides along: `malloc`→arg0, `realloc`→arg1, `free`→null, alloca→element count × type size, GEP→`base - offset` (`:917-955`, `:1131-1160`), via `ObjectSizeOffsetEvaluator` (`getSizeForValue`, `:193`). **But this size is allocation size for bounds checks; it is never wired into the IDL as a marshalling length.**

**Fills:** the *kind* axis of `lind_arg_spec`: Safe→SCALAR-pointee/CONST/struct-layout; Seq→array (`FROM_ARG`-ish, but length unresolved → `size_unknown` warning); Dyn→**residue/flag** (no clean Lind mapping; closest is `LIND_RET_FORCE_LOCAL` / manual).

### 2.5 Strings, sentinels, special memory

- **String (`LIND_SIZE_CSTR`).** `isFieldUsedInStringOps` (`SharedDataAnalysis.cpp:400`): follow `PARAMETER_IN`/`DATA_ALIAS` edges from the node; if any reachable value is passed to a known string function (`_string_op_names` = `strcpy,strncpy,strlen,strlcpy,strcmp,strchr,strncmp,strpbrk,kobject_set_name`, `:43-54`) ⇒ it's a string. Root-level `char*` is also checked directly (`inferTreeNodeAnnotations`, `:1346-1352`). Adds `"string"` annotation → IDL type becomes `string *` (`patchStringAnnotation`, `:27`). **Confirms the paper's "string-from-use" + library-hint approach.** Maps to `LIND_SIZE_CSTR`.
- **Sentinel-terminated arrays.** Detected *outside* this code: `BoundaryAnalysis` looks at global op-struct initializers and records fields whose initializer is a "user of sentinel-type val" (`isUserOfSentinelTypeVal`, `BoundaryAnalysis.cpp:144`) into `sentinel_fields`. `SharedDataAnalysis::readSentinelFields` (`:521`) loads them; `computeDataAccessForTreeNode` sets `is_sentinel` + forces a READ tag (`DataAccessAnalysis.cpp:367-371`). IDL emits `array<T, null>` (`:688`). No Lind analog yet (we'd want a `LIND_SIZE_SENTINEL`).
- **User memory / ioremap.** `inferUserAnnotation` (`:1270`): if an alias flows into `_copy_from_user`/`_copy_to_user`, emit `user<{{N}}>` and try to recover the constant byte count from the 3rd arg by chasing trunc→load→store-of-constant (`:1293-1316`). `handleIoRemap` (`:1042`) tags `void*` returns of `ioremap*`/`pci_iomap` with `ioremap(caller)`. These are the only places a **dynamic size for a `void*` buffer** is recovered — and only for the two `copy_*_user` intrinsics, not general `(buf,len)`.

### 2.6 Nested / recursive structs, unions, container-of

- **Nested struct** → recursive `generateIDLFromTreeNode` (`:766`) emits a nested `projection`. Self-reference (linked list) is special-cased: if a struct-pointer field's lowest type name equals the root type name, emit `projection T* field;` without recursing (`:711-719`). So **cycle termination at IDL time is name-equality, not a visited set.**
- **Unions.** Layout is reconstructed from DWARF elements like any composite, but the **active arm is not resolved** — the union branch (`:756-762`) literally has a commented-out `[anon_union]` and a `TODO: need to resolve union types`. Anonymous unions are emitted inline (`:801-807`). **Confirms KSplit is weak on unions; no discriminator inference.** Maps to our `LIND_LO_UNION` + `discriminator_field`, which KSplit would leave manual.
- **Container-of / `within`.** `computeContainerOfLocs` (`:1503`) finds **negative-offset GEPs** (`getGEPAccessFieldOffset < 0`) followed by a bitcast to a shared struct type, and only **counts stats** (`_shared_containerof++`, `:1534`). The `inferMayWithin` (`:1231`) that would emit a `may_within<...>` attribute is **defined but its call site is commented out** (`:672`). So in this checkout, `within`/`container_of` is *detected for stats but not emitted* — it is effectively residue.

### 2.7 Alloc/dealloc ownership

`propagateAllocSizeAnno` (`:195`) reads allocator calls (`_PDG->getAllocators()`), builds `alloc<{{size,gfp}}>`, and pins it to the parameter tree node that aliases the allocation — `(caller)` if it escapes into a param tree locally, `(callee)` if it crosses the boundary (`findCrossDomainParamNode`, `:136`). `inferDeallocAnno` (`:262`) does the backward version for frees. Maps loosely to our handle-table alloc/free, but again it's RPC-side-allocation, not handle tokens.

### 2.8 Boundary identification (`BoundaryAnalysis.cpp`) → our interface function set

**Plain English.** The "boundary" = functions that cross the kernel/driver line. KSplit derives it from the **driver↔kernel communication idiom**, not from analysis:
- **Imported funcs** (driver→kernel calls) = declared-but-not-defined functions minus a blacklist (`computeDriverImportedFuncs`, `:51`; blacklist from `liblcd_funcs.txt`, `:31`).
- **Exported funcs** (kernel→driver calls) = function pointers stored into **driver global op-structs** (e.g. `*_operations`), found by walking global initializers and matching func-pointer fields (`computeExportedFuncs`, `:80-159`).
- **Exported symbols** = globals named `__ksymtab_*` / `__kstrtab_*` (`computeExportedFuncSymbols`, `:166`).
- Plus the module `_init_module` function (`SharedDataAnalysis::getModuleInitFunc`, `:620`).
All dumped to files (`:182`) and re-read by `SharedDataAnalysis::setupBoundaryFuncs` (`:92`). `computeBoundaryTransitiveClosure` (`:133`) expands to everything reachable in the call graph — this is the set over which access analysis must run to be sound.

**Lind mapping:** our "interface function set" = the exported library symbols of a `.so` (zlib's `deflate`, `compress2`, …). KSplit's op-struct/ksymtab heuristics are kernel-specific; for us the boundary is simply the dynamic-symbol export table — *much easier than KSplit's job.*

---

## 3. Mapping table: our spec field → KSplit mechanism → file:line

| Our `lind_marshal.h` field | KSplit mechanism | file:line |
|---|---|---|
| interface function set (which fns to wrap) | `BoundaryAnalysis` imported/exported/ksymtab + transitive closure | `BoundaryAnalysis.cpp:51,80,166`; `SharedDataAnalysis.cpp:133` |
| `lind_arg_kind = SCALAR` | non-pointer DWARF type (no tree expansion past leaf) | `Tree.cpp:40` |
| `lind_arg_kind = PTR` | pointer DIType expanded to pointee child | `Tree.cpp:43-50` |
| `lind_arg_kind = HANDLE` | **none — no analog** | — |
| `lind_ptr_direction = IN` | READ access tag (`LoadInst`/GEP-base) → `"in"` | `PDGUtils.cpp:142`; `DataAccessAnalysis.cpp:1387` |
| `lind_ptr_direction = OUT` | WRITE access tag (`StoreInst`/write-libcall) → `"out"` | `PDGUtils.cpp:157`; `DataAccessAnalysis.cpp:1389` |
| `lind_ptr_direction = INOUT` | both tags present | `Tree.hh:39-40` |
| `lind_size_kind = CONST` | CCured **Safe** singleton; `sizeof(*p)` from DWARF / `ObjSizeEval` | `NesCheck.cpp:851`; `AnalysisState.cpp:20` |
| `lind_size_kind = FROM_ARG` (buf,len) | **dataflow-only**: length = the size operand of a recognized copy sink (`copy_*_user` 3rd arg, constant-folded); **no heuristic sibling-arg pairing**; runtime-variable / loop-bound lengths → `array<T,size_unknown>` + human | `DataAccessAnalysis.cpp:1293`,`:703` |
| `lind_size_kind = FROM_ARG_POINTEE` | **partial** — only the `_copy_*_user` 3rd-arg constant recovery; no general `*p->len` | `DataAccessAnalysis.cpp:1293` |
| `lind_size_kind = CSTR` | `isFieldUsedInStringOps` → `"string"` | `SharedDataAnalysis.cpp:400`; `DataAccessAnalysis.cpp:1346` |
| (sentinel arrays) | initializer scan → `is_sentinel` → `array<T,null>` | `BoundaryAnalysis.cpp:144`; `DataAccessAnalysis.cpp:688` |
| `lind_layout` (struct tree) | parameter tree BFS expansion (DWARF-driven) | `Tree.cpp:31,156`; `FunctionWrapper.cpp:37` |
| `lind_field.offset` | **deferred to DWARF**; matched via `isGEPOffsetMatchDIOffset` | `Tree.cpp:106`; `PDGUtils.cpp:83` |
| `lind_field.touched` (projection) | shared-field-id ∩ has-access-tag, minus opt-out | `SharedDataAnalysis.cpp:436`; `DataAccessAnalysis.cpp:632,646,656` |
| `lind_layout_kind = UNION` + discriminator | layout reconstructed; **arm/discriminator unresolved (TODO)** | `DataAccessAnalysis.cpp:756` |
| `lind_return_kind = VOID/SCALAR` | return value tree + access tags (in-stripped) | `FunctionWrapper.cpp:75`; `DataAccessAnalysis.cpp:680` |
| `lind_return_kind = PTR_ALIAS_ARG / PTR_INTO_ARG` | **none** (no return-aliases-arg inference) | — |
| `lind_return_kind = HANDLE` | **none** | — |
| `lind_return_kind = FORCE_LOCAL` | conceptually = Dyn/wild residue + warnings | `NesCheck.cpp:746`; `SharedFieldsAnalysis.cpp:216` |
| alloc/free ownership | `alloc<...>(caller/callee)` / `dealloc(caller)` | `DataAccessAnalysis.cpp:195,262` |

---

## 4. Gaps: what KSplit does NOT compute (that we still need)

1. **Opaque handles (our #1 need).** Zero support. No pass-by-token, no handle table, no "never dereference" class. Every pointer is copied or flagged. `LIND_ARG_HANDLE`, `LIND_RET_HANDLE`, `handle_class` have **no KSplit source analog.** This must come from elsewhere (accelerator-remoting clusters) or be hand-annotated.
2. **Heuristic `(buf, len)` arg pairing / runtime-variable `FROM_ARG`.** KSplit recovers a length only from **visible dataflow** — the size operand of a recognized copy call (`copy_*_user`, constant-folded; `:1293`) or an allocation (`ObjSizeEval`; `NesCheck.cpp:193`) — and emits `array<T, size_unknown>` (`:703`) when neither is visible, leaving it to a human. The allocation size feeds *pointer classification + runtime bounds*, not the IDL array length. What KSplit does **not** do: (a) guess the length from a sibling integer argument's type/position, nor (b) encode a runtime-variable "buffer length = argument N" dependency in the IDL. Our `LIND_SIZE_FROM_ARG` does both — the `acc.lengths` path mirrors KSplit's copy-operand recovery *and* encodes the arg dependency, and we add a heuristic sibling-arg guess for the case KSplit punts on. The classic punt case is a **loop bound**: `adler32(seed, buf, len)`'s `for(i<len) buf[i]` has no bulk copy call (so the copy-sink matcher finds nothing) and `buf` is a parameter (so `ObjSizeEval` returns unknown) — KSplit emits `size_unknown` there; we guess (see §6).
3. **Return-value aliasing of an argument** (`LIND_RET_PTR_ALIAS_ARG` / `PTR_INTO_ARG`, e.g. `strchr` returns a pointer into its arg). No inference; KSplit treats the return as its own tree.
4. **wasm32 offset remapping.** KSplit keeps **native/DWARF offsets and sizes** throughout (`Tree` stores `DIType`; sizes via `ObjectSizeOffsetEvaluator` on the host `DataLayout`, `NesCheck.cpp:196`). For Lind the descriptor must use **wasm32 (ILP32) layout**: 4-byte pointers, different struct padding. We'd reuse KSplit's *which fields are touched* but recompute every `offset`/`struct_size` against the wasm32 `DataLayout` — i.e. emit the descriptor from a wasm-target compile, or post-process DWARF offsets through a wasm32 ABI model.
5. **`within`/container-of attribute emission** is present-but-disabled in this checkout (`inferMayWithin` call site commented at `DataAccessAnalysis.cpp:672`); only stats are collected (`:1503`).

**Residue / warning mechanism (the "infer common case, flag the rest" core).** KSplit's safety net is *flag-don't-guess*, surfaced three ways:
- **`errs()` warnings to stderr** — the actual residue channel a human reads:
  - non-void wild (Dyn) pointers: `"Find non-void wild ptr"` (`NesCheck.cpp:752`).
  - unsafe struct→struct casts that may hide shared fields: `"potential wild casting that may cause missing shared fields"` + `_num_wild_cast` counter (`SharedFieldsAnalysis.cpp:216-217`).
  - variadic-index GEP into a struct pointer: `"[Warning]: GEP has variadic idx. May miss field access"` (`DataAccessAnalysis.cpp:426`).
- **In-IDL `size_unknown` / `null` tokens** — a seq pointer with no recoverable length emits `array<T, size_unknown>` (`:692-705`); the IDL compiler / human must fill the size. This *is* the "flag the array we couldn't size" output.
- **Silently-correct conservative defaults** — a field with no access tag is simply omitted (under-copy risk if analysis missed an access; this is the documented 7-misclassification trap), and unresolved unions are emitted as raw projections with a `TODO`.

Criteria, in one line: **anything that is (a) a type-incompatible cast (Dyn), (b) a sized array whose length isn't statically a constant/allocation, (c) a union active arm, or (d) a container-of/negative-GEP gets flagged rather than marshalled.**

---

## 5. Concrete takeaways for reimplementing this for wasm32 libraries

1. **Keep the spine, drop the kernel scaffolding.** The reusable core is exactly three things: parameter trees (`Tree.cpp`), READ/WRITE→direction (`PDGUtils.cpp:142/157` + `DataAccessAnalysis.cpp:351`), and shared∩accessed→`touched` (`SharedFieldsAnalysis.cpp:260` / `SharedDataAnalysis.cpp:436`). Everything bootstrapped from plaintext driver files (`driver_funcs`, op-structs, ksymtab) is kernel-idiom-specific and should be **replaced by reading the `.so` dynamic export table** for the boundary set — far simpler than `BoundaryAnalysis.cpp`.
2. **Our "both sides" is asymmetric.** KSplit's shared-field test needs *both* domains' IR (`set_intersection` of driver+kernel accesses). We only have the **library** side, not arbitrary app callers. Two options: (a) treat *every accessed field* as touched (over-copy, always correct — drop the intersection, keep only the access tag), or (b) approximate "the other side" by the library's own public API contract. Recommend (a) for correctness, since the over-copy is bounded by the projection to accessed fields anyway.
3. **Direction inference transfers cleanly** — it only needs the callee (library) IR, which we have. Port `hasReadAccess`/`hasWriteAccess` almost verbatim; add wasm-relevant write-libcalls to `dataWriteLibFuncs` (memcpy/memset/etc.).
4. **Recompute every offset/size on the wasm32 target.** Compile the library to wasm32, run the analysis on *that* module's `DataLayout`, so `lind_field.offset` and `lind_layout.struct_size` are wasm32-correct. Do **not** reuse host DWARF offsets. The *touched-field decision* is layout-independent and can be computed on either build; the *numbers* must be wasm32.
5. **Match KSplit's dataflow length recovery, then go beyond it.** KSplit recovers a length from a recognized copy sink's size operand (`copy_*_user`) or an allocation, else `size_unknown`+human. Our `acc.lengths` path is the same idea (length operand of `memcpy`/known libcalls → trace to an arg), extended to (a) encode the arg dependency as `LIND_SIZE_FROM_ARG(idx)` and (b) a heuristic sibling-arg guess for the loop-bound case KSplit punts on. Two things KSplit's mechanism does *not* reach that we'd still want: the **loop-bound** length (`adler32`; needs `ScalarEvolution`/trip-count analysis to tie the GEP bound to the arg) and the `*p->len` indirection (compress2's `*destLen` → `FROM_ARG_POINTEE`). Per KSplit's discipline, gate the *heuristic* (not the dataflow) result behind a warning / `force_local` so a wrong guess never silently ships.
6. **Adopt the flag-don't-guess discipline explicitly.** Mirror KSplit: Dyn/wild pointer → emit `LIND_RET_FORCE_LOCAL` (or a `LIND_ARG` "manual" marker) + a stderr warning; unresolved array length → emit a `size_unknown` sentinel the human fills; union → emit layout but require a hand-written `discriminator_field`. Aim for KSplit's ~98%-auto / 2%-manual operating point, not 100%.
7. **Handles are entirely on us.** No KSplit code path produces `LIND_ARG_HANDLE`. Plan to detect "pointer returned by a constructor-shaped fn and only ever passed back opaquely, never deref'd through GEP/load" as a *new* heuristic, or annotate by hand. This is the biggest delta from KSplit and matches the paper-note verdict.
