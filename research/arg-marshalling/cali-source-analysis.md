# CALI — Source-Level Analysis (user-space PDG library isolation)

Source: `/home/lind/inference-refs/Cali-library-isolation`, the artifact for
Bauer & Rossow, *"Cali: Compiler Assisted Library Isolation"* (AsiaCCS'21).
Analysis covers `cali-linker/` (the LLVM link-time engine), `libipc/` (runtime
RPC/shared-memory/callback support), and `instrumentations/` (per-library debug
hooks, **not** marshalling descriptors). The bundled `glibc-2.23/`,
`precompiled-libraries/`, `libnsjail/` are ignored as instructed.

> **Headline correction to our secondary notes.** `report.md` §4.5 lumps CALI
> with PtrSplit/KSplit ("parameter tree + selective pointer-bounds tracking +
> deep-copy plan"). **The real CALI engine does none of that.** CALI never
> computes per-argument sizes, copy directions, or a transitive deep-copy plan,
> and it never serializes pointee bytes. Instead it puts **all data that can
> reach the boundary into a shared `mmap` region at the same virtual address in
> both processes**, and passes arguments by *bit-copying the raw argument struct*
> (pointers included) across an SHM mailbox. The PDG is used to decide **which
> allocations must be promoted to shared memory**, not to describe how to copy
> them. This makes CALI a *shared-address-space* model, not a *copy-across*
> model — a crucial difference for us (see §5).

---

## 1. Architecture: the link-time pipeline (`ld.cali`)

CALI is a drop-in replacement linker (`-fuse-ld=cali`, installed as `ld.cali`,
README "How to use"). The program is built with `-flto` so the linker sees LLVM
bitcode for the whole program *and* the to-be-isolated library. A YAML policy
(`--cali-config=...`) partitions object files into **contexts** (`main`,
`library`) by glob selectors (`config.all.yaml:17-34`).

Driver / module flow:
- `cali-linker/cali_linker/linker_replacement.cpp` — entry: parses the real `ld`
  command line, loads config, builds `LlvmIpcModule`s.
- `cali-linker/modules/llvm_module.cpp:41-55` — `findImportsExports()` classifies
  every defined/declared symbol per context into `imports`/`exports`.
- `cali-linker/cali_linker/ipc_rewriter.cpp:37-60` — `createCommunicationPair`:
  the set of symbols to turn into RPC is the **set-intersection of one context's
  imports with the other's exports** (`ipc_rewriter.cpp:53-56`). That intersection
  *is* the boundary interface — it is computed purely from the linker symbol
  tables, **not** from the PDG.
- The PDG passes then run over each context's module to decide shared memory and
  to specialize functions. `communication_pair.cpp` emits the RPC stubs.
- A separate `communication-*.bc` module is generated holding the dispatch
  switch (`ipc_rewriter.cpp:66-78`), linked with `libipc`.

Pass ordering (LLVM `RegisterPass` + `getAnalysisUsage` dependencies):
`DataDrivenSCCPass` → `PDGCreationPass` → `PDGReachabilityPass` →
`PDGSpecializationPass` → `PDGSharedReachabilityPass` → `PDGSharedMemoryPass`
(`PDGCreationPass.cpp:67-72`, `PDGReachabilityPass.cpp:22-27`,
`PDGSpecializationPass.cpp:21-28`, `PDGSharedMemoryPass.cpp:15-17`,
`PDGSharedReachabilityPass.cpp:62-71`). `PDGInstrumentPass` is an optional
statistics pass (`PDGInstrumentPass.cpp:17` — registration commented out).

---

## 2. PDG construction + data-flow analysis

### 2.1 The graph type
`PDG` is a boost-style graph of `PDGNode`/`PDGEdge` (`passes/PDG.h`). A `PDGNode`
(`PDG.h:11-25`) carries the LLVM `Value*`, its `Type*`, and the boolean lattice
that the whole analysis turns on:
- `source` / `source_type` — node is a memory allocation (1 = malloc/alloca/global,
  2 = `posix_memalign`-style out-param).
- `ipcsink` — node's *memory* crosses the IPC bridge (an argument pointee of a
  boundary call).
- `reaches_ipc_sink` — node can flow (backwards) into an `ipcsink`.
- `influenced_by_shared` — node can be affected by writes to shared memory
  (forward taint, used only for the security warning in §4).
- `filedescriptor` — node holds an fd.

`PDGEdge::PDGEdgeType` (`PDG.h:27-43`) is the key vocabulary:
`Data` (def→use flow), `Part`/`Deref` (**structural** edges = the parameter-tree
analog), `Param`/`Return` (call/signature wiring), `Invocation` (fn-ptr → call),
and `Summary_*` (cross-SCC summarized flow).

### 2.2 The parameter-tree analog: `createSubnodes`
For every value, `PDG::createSubnodes` (`PDG.cpp:10-57`) recursively unfolds the
*type*: a pointer gets one `Deref` child (`PDG.cpp:23-33`); a struct gets one
`Part` child per field (`PDG.cpp:34-55`). Recursion depth is capped by
`subnode_limit` (`config.limit_subnode_depth`, default 3) and struct width by
`max_parts` (default 32; fields beyond are merged into a single overflow node,
`PDG.cpp:48-53`). **This is exactly our `lind_layout`/`lind_field` tree** — a
structural unfolding of pointer + struct types — except CALI builds it from the
LLVM type alone and uses it only for taint propagation, never to drive a copy.

### 2.3 Building data-flow edges
`PDGCreationPass::addEdges` (`PDGCreationPass.cpp:142-343`) walks each
instruction:
- `load`/`store` connect a pointer's `Deref` subnode to the loaded/stored value
  via `addSubnodeEdges` (`PDGCreationPass.cpp:150-178`).
- `getelementptr` maps constant indices onto `Part` subnodes
  (`PDGCreationPass.cpp:231-276`), giving **field-precise** flow.
- `bitcast` unifies subnodes; a struct→first-member cast is treated GEP-style
  (`PDGCreationPass.cpp:277-302`).
- `call` adds `Param` edges arg→call and an `Invocation` edge for indirect
  callees (`PDGCreationPass.cpp:179-214`).
- `memcpy`/`realloc` are modeled as known summaries connecting src/dst pointee
  subnodes (`PDGReachabilityPass.cpp:270-285`).

`addSubnodeEdges` / `uniteSubnodes` (`PDG.cpp:150-316`) implement a
**field-sensitive unification**: data flow on a pointer recursively unifies its
`Deref` child (one level becomes two-directional/"constant" via `constantLevels`,
`PDG.cpp:169-180`), and identical struct fields are merged. This is CALI's
points-to: alias = "nodes united in the PDG."

### 2.4 Interprocedural: SCCs + summary edges
`DataDrivenSCCPass` computes the call-graph SCCs. `PDGReachabilityPass::runOnSCC`
(`PDGReachabilityPass.cpp:51-143`) processes callee-before-caller. Same-SCC calls
get direct param/return edges (`PDGReachabilityPass.cpp:162-177`); cross-SCC calls
get **summary edges** (`Summary_Data`, `Summary_Invocation`) computed by
`calculateSummaryEdges` (`PDGReachabilityPass.cpp:348-489`), which BFS-summarizes
how a callee's parameters reach its return/other params so callers don't re-walk
the body. This is a modular, bottom-up data-flow — the same idea PtrSplit/KSplit
use, but here it carries *taint bits*, not copy descriptors.

---

## 3. How CALI answers our five marshalling questions

### 3.1 Is-pointer? (which arguments need handling)
CALI does **not** classify each argument's *kind*. The only argument-level
decision is the symbol-table boundary set (§1). The "is this a pointer to data
that must be visible across the boundary" question is answered indirectly:
`markIpcCall` (`PDGReachabilityPass.cpp:548-560`) takes every boundary-call
instruction, adds `Param` edges for its operands, and calls
`markAsPointerToShared` on each (`PDGUtilities.cpp:145-152`), which walks the
operand's `Deref`/`Part` subtree marking the **pointees** `ipcsink`
(`markAsShared`, `PDGUtilities.cpp:154-162`). So a scalar argument touches nothing;
a pointer argument marks its whole transitively-reachable pointee tree as "must
live in shared memory." There is no SCALAR/PTR/HANDLE enum — the distinction
falls out of whether the subtree has `Deref` children.

### 3.2 Region / buffer extent (the analog of our FROM_ARG / size_is)
**CALI does no buffer-size inference at all — and does not need to.** Because the
pointee already lives in shared memory mapped at the same address in both
processes, the callee dereferences the *original pointer*; nothing is copied, so
no length is required. The only place a size is ever materialized is when
*promoting an allocation* to shared memory, and there the size is simply the
allocation's own size:
- `alloca`: `getTypeAllocSize` of the allocated type
  (`PDGSharedMemoryPass.cpp:175-202`).
- `malloc`/`calloc`: the original size argument(s) are forwarded unchanged to
  `shm_malloc`/`shm_calloc` (`PDGSharedMemoryPass.cpp:166-173`,
  `PDGInstrumentPass.cpp:138-148`).
- globals: `getTypeAllocSize` and relocation into a `shm_data` section
  (`PDGSharedMemoryPass.cpp:204-231`).

So CALI **delegates "how many bytes" to the allocator/type system**, never to a
size-expression inferred from a sibling argument. There is no `FROM_ARG`,
`FROM_ARG_POINTEE`, or `CSTR` analog anywhere in the engine.

### 3.3 Direction (in vs out — the analog of our IN/OUT/INOUT)
**Also absent**, for the same reason: with shared memory there is no copy-in or
copy-out, so direction is irrelevant to correctness. The closest concept is the
*security* taint `influenced_by_shared` (`PDGSharedReachabilityPass.cpp:33-37`):
a forward BFS over `Data`/`Part`/`Deref`/`Summary_Data` edges from shared nodes,
used only to **warn** when a value derived from attacker-writable shared memory is
used as an indirect-call target (`warnForPossiblyDangerousActions`,
`PDGSharedReachabilityPass.cpp:249-288`). It does not gate any copy. So CALI has
no IN/OUT inference whatsoever.

### 3.4 Nested pointers / deep copy
The PDG *represents* arbitrarily nested pointers (recursive `Deref`/`Part`,
§2.2) and `markAsShared` recurses through the whole subtree
(`PDGUtilities.cpp:154-162`), so a `struct { char *buf; struct *next; }` reachable
from a boundary argument has every transitively-reachable allocation promoted to
shared memory. **But there is no deep *copy*** — the entire object graph is simply
allocated in shared memory and dereferenced in place. Cycles are handled
trivially by the `ipcsink`/visited guards in the recursions (e.g.
`markAsShared`'s `if (!graph[v].ipcsink)` early-out, `PDGUtilities.cpp:155`;
`isShared`'s `visited` set, `PDGSpecializationPass.cpp:117-131`). The depth cap
`subnode_limit` bounds tree construction; beyond it, conservative unification
keeps soundness.

The hard part CALI *does* solve is **propagating "must be shared" through
indirection**: if a function `*out = malloc()` and `out` reaches the boundary,
the malloc must be shared even though the malloc is several calls away. This is
the job of **`PDGSpecializationPass`** (`PDGSpecializationPass.cpp`): when a
function is called in a context where one of its pointer params/returns reaches a
sink, CALI **clones the function** (`__ipc_specialized_` prefix,
`getSpecializedName` `:333-335`; `cloneSCC` `:202-261`), re-runs reachability on
the clone (`:75-77`), and rewrites just the tainted call sites to the clone
(`specializeCalls` `:472-551`). The clone's internal allocations become shared;
the unspecialized version stays private. `matchAndPropagateTaint`
(`:337-396`) is the param-tree matcher that copies `reaches_ipc_sink` from
call-site arg subnodes onto the callee's parameter subnodes — structurally
identical to our recursive field walk, but moving a bit instead of bytes.

### 3.5 Opaque pointers / handles / callbacks / function pointers
This is where CALI has *real, working* machinery, all in `libipc` at runtime
plus type-driven detection at link time:
- **Opaque handles (e.g. `sqlite3*`, `z_streamp`):** handled "for free" by the
  shared-memory model — the object lives in SHM, both sides hold the same
  pointer, no translation table needed. (Contrast our `LIND_ARG_HANDLE` token
  table, which exists only because we have separate address spaces.)
- **Callbacks / function pointers across the boundary:** detected purely by LLVM
  *type* at the stub generator — `wrapOutgoingValue`/`wrapIncomingValue`
  (`communication_pair.cpp:467-508`) fire when
  `value->getType()->getPointerElementType()->isFunctionTy()`. Outgoing: the fn
  pointer is replaced by an integer index into a per-process callback table
  (`wrap_outgoing_callback`, `libipc/ipc_communication.cpp:360-369`). Incoming:
  the other side builds an **executable x86-64 trampoline** (`mov r11,idx; jmp
  replacement`) so that when the library calls the "callback" it actually
  re-enters the IPC bridge and the call is dispatched back to the owning process
  (`wrap_incoming_callback`, `ipc_communication.cpp:373-407`;
  `CallbackManager::execute`, `:410-418`). The PDG's `Invocation`/`Summary_Invocation`
  edges (`PDGReachabilityPass.cpp:224-238`) and `markIpcCall` on indirect calls
  (`calculateIpcFunctionCalls`, `:314-344`) ensure callback parameters are also
  recognized as boundary-crossing so their captured data is shared.
- **`va_list`:** special-cased and cloned across the bridge
  (`communication_pair.cpp:475-482`, `wrapOutgoingVaList`).
- **File descriptors:** a genuine inference. `PDGUtilities.cpp:122-143` seed fd-ness
  from known libc functions (`open`/`socket`/`read`/`close`/`__fxstat`/`pipe`);
  `calculateFDReachability` (`PDGUtilities.cpp:56-120`) forward-propagates the
  `filedescriptor` bit along `Data` edges (with a struct-field heuristic,
  `:90-108`); `checkForFileDescriptorParameters`
  (`PDGSharedMemoryPass.cpp:233-260`) records which **boundary-function argument
  indices** are fds into `config->filedescriptors`. At the stub, those args are
  passed through `ipcShareFD` (SCM_RIGHTS fd-passing,
  `communication_pair.cpp:186-207`). This *is* a per-argument descriptor that gets
  serialized into the generated config — the only place CALI emits arg-level
  metadata, and the closest analog to our marshalling spec.

---

## 4. Residue / uncertainty strategy

CALI's design point is **"share conservatively, never copy."** When analysis is
uncertain it does not fall back to copy-everything or annotations — it **shares
more memory** (sound for functionality; weaker for isolation):

1. **Conservative unification.** Any flow it can't resolve precisely is modeled as
   two-way data flow / node unification (`uniteSubnodes`, `PDG.cpp:191-316`),
   which can only *grow* the shared set.
2. **Type/width caps degrade to over-approximation.** Structs wider than
   `max_parts` collapse into one overflow node (`PDG.cpp:48-53`); depth past
   `subnode_limit` stops unfolding — both make the node coarser/more-shared, never
   unsound.
3. **Indirect calls.** Without `strongFPAnalysis` (default off,
   `config.all.yaml:11`) indirect callees aren't followed for specialization;
   the indirect-call value is checked for shared influence and, if tainted, a
   **runtime warning** is emitted (`warnForPossiblyDangerousActions`,
   `PDGSharedReachabilityPass.cpp:249-288`) rather than failing the build.
4. **`function_behavior` config map** is the one *annotation* knob: it lets the
   user declare that an opaque allocator (e.g. ImageMagick's
   `AcquireMagickMemory`) behaves like `malloc`/`free`, so its results are shared
   and its `free` is excluded from taint (`PDGCreationPass.cpp:44-57`,
   `getSharedMemoryFunction` `PDGSharedMemoryPass.cpp:134-164`,
   `considerFunction` `PDGSharedReachabilityPass.cpp:234-247`). `filedescriptors`
   and `replaceArguments` are the other user-supplied maps
   (`config.all.yaml:12-14`, HOWTO table).
5. **Runtime safety net.** `mprotect_mode`/`sequential_mode`
   (`config.all.yaml`, HOWTO) protect shared memory against concurrent
   TOCTOU attacks rather than statically proving directions.

The `instrumentations/*.h` headers (`lib-sqlite3.h`, `lib-zlib.h`, …) are **not**
marshalling specs — they are optional `instrument_before_/after_` **debug/validation
hooks** wired in via `instrument_user` (`communication_pair.cpp:262-282`). They
print and `check_ptr()` arguments to verify pointers really landed in shared
memory; they carry no size/direction data.

---

## 5. Config / policy file format (example)

`cali-linker/sample_configs/config.all.yaml` is the canonical template. The
salient parts for us:

```yaml
limit_subnode_depth: 3        # parameter-tree depth cap (our layout recursion limit)
limit_struct_parts: 32        # struct fanout cap
strongFPAnalysis: false       # follow indirect calls during specialization
strongFDAnalysis: false       # struct-field fd heuristic
filedescriptors: {}           # {func_name: [arg_indices]} — fd-passing policy
contexts:
  main:
    selectors: ["*.o", "libstdc++.a"]
    function_behavior: {}      # {AcquireMagickMemory: malloc, ...}
  library:
    selectors: ["libabc.so"]
    permissions: { ... nsjail sandbox: fs, user, seccomp, rlimits ... }
```

Real per-app configs (`config-socat.yaml`, `config-imagemagick.yaml`,
`config-filezilla.yaml`) mostly add `function_behavior` allocator aliases and the
nsjail permission policy. **There is no per-function argument-marshalling
descriptor in any config** — confirming sizes/directions are never specified.

---

## 6. Runtime marshalling (`libipc`) — how arguments actually move

Generated by `CommunicationPair::generateCommunicationFunction`
(`communication_pair.cpp`, ~`:100-294`) and its handler counterpart
(`createReplacementHandler`, `:304-390`):

1. Caller stub grabs the SHM mailbox (`getSendingStruct`, `:213`), builds an
   anonymous `StructType` of the **raw parameter types**
   (`:214-218`), and **`store`s each argument verbatim** into that struct in SHM
   (`:223-238`). Pointers are stored as-is — valid because the pointee is also in
   SHM at the same address. The only transforms are callback-wrapping,
   va_list-cloning (§3.5), and fd-sharing.
2. Writes a per-function integer `code`, signals `trigger_ipc_call`, and blocks on
   `ipc_wait_for_return` (`:241-248`).
3. Callee process (`ipc_communication.cpp` event loop, `:127-177`) reads the
   `code`, jumps to the generated `switch` case, **`load`s the same struct**
   (`:336-349`), unwraps callbacks (`wrapIncomingValue`, `:349`), calls the real
   library function (`:355`), stores the result back into SHM, and returns
   (`:355-390`).
4. SHM/allocator: `libipc/shm_malloc.h` + `libipc/malloc/` (a renamed glibc
   malloc operating over a shared `mmap` arena, mapped at a fixed common address);
   `shm_malloc`/`shm_calloc`/`shm_free`/`realloc_to_shm`/`posix_shm_memalign`/
   `shm_mmap_share` are the promoted-allocation entry points
   (`PDGSharedMemoryPass.cpp:116-128`).

So "serialization" is a **single `memcpy`-equivalent struct store of the raw ABI
arguments** — no field-by-field packing, no pointee copy.

---

## 7. CALI vs KSplit — and which is the better model for us

**Where CALI fits us better than KSplit.**
- *User space, no kernel, no hypervisor.* CALI is a plain LLVM linker producing
  ordinary processes + nsjail; KSplit splits a monolithic *kernel* with kernel
  IDL glue. Our cages are user-space sandboxes — CALI's "two processes + RPC
  mailbox + promoted allocations" is structurally our "two cages + 3i dispatch +
  copy region."
- *Fully automatic boundary discovery.* CALI derives the interface from
  imports∩exports at link time (§1) with **zero IDL** — the dream end-state for
  our inferencer. KSplit still emits an IDL that a human reviews.
- *Callbacks/fn-pointers actually work* via index+trampoline (§3.5). This is our
  #1 hard case and CALI ships a concrete, type-driven solution we can adapt
  (replace the x86 trampoline with a grate-registered dispatch shim).
- *Opaque handles need no token table* in CALI — instructive: handles only need
  translation because we have *separate* linear memories.

**Where CALI is weaker / does less than we need.**
- *No size inference, no direction inference, no deep-copy plan.* These are
  exactly the descriptors `lind_marshal.h` is built around (CONST/FROM_ARG/
  FROM_ARG_POINTEE/CSTR; IN/OUT/INOUT). CALI sidesteps all of them by sharing
  memory, which **we cannot do** across cages with separate linear memories.
  KSplit (and PtrSplit, its ancestor) *do* infer sizes (`size_is`/bounds
  tracking), directions (`in`/`out` from read/write analysis), and projections —
  and are therefore the better model for our *copy-across* mechanics.
- *Field-level projection ("touched").* CALI promotes whole reachable objects;
  KSplit's projection copies only fields actually used. Our `lind_field.touched`
  is the KSplit idea, not CALI's.
- *Net:* **CALI is the better architectural/automation model (user-space,
  link-time, imports∩exports, callbacks); KSplit is the better marshalling-
  semantics model (size/direction/projection).** Our inferencer wants CALI's
  front end feeding KSplit-style descriptors.

---

## 8. Concrete lessons for the wasm-cage inferencer

1. **Reuse CALI's PDG shape verbatim as our parameter tree.** `Deref` + `Part`
   subnodes with `subnode_limit`/`max_parts` caps (`PDG.cpp:10-57`) map 1:1 onto
   `lind_layout`/`lind_field`. We already converged on the right structure.
2. **Boundary = imports∩exports at link/registration time** (`ipc_rewriter.cpp:53-56`).
   For us: the set of symbols a cage imports that the grate exports. This is a
   cheap, exact way to enumerate which functions need specs — do this first.
3. **But we *must* add the size/direction layer CALI skips.** Because we copy
   across separate linear memories, every `Deref` edge that CALI would resolve "by
   sharing" we must resolve "by knowing the byte count and the direction."
   Borrow that layer from KSplit/PtrSplit (read/write analysis → IN/OUT;
   `size_is`/`malloc`-size/`strlen`-use → our size kinds). CALI gives us the
   *graph*; KSplit gives us the *labels on the edges*.
4. **Steal CALI's specialization trick for transitive `*out = malloc()`.**
   `PDGSpecializationPass` (clone + re-run reachability + rewrite tainted call
   sites, `:202-261`,`:472-551`) is exactly how to discover that a buffer
   allocated deep inside the library must be marshalled back — our
   `RET_FORCE_LOCAL`/OUT-pointer cases. Adapt it to flag "this allocation escapes
   to the source cage → needs copy-out + pointer re-translation."
5. **Adopt CALI's callback solution, retargeted.** Detect fn-pointer params by
   type at spec-gen time (`communication_pair.cpp:467-508`); register the source-
   cage callback in a per-grate table (like our handle table) and hand the library
   an index/shim that re-enters 3i — the grate analog of CALI's trampoline. This
   directly attacks our SQLite/OpenSSL callback hard case.
6. **Infer fd arguments the CALI way.** Seed from known syscalls, forward-propagate
   a `filedescriptor` bit, and emit per-arg-index fd metadata
   (`PDGUtilities.cpp:122-143`, `PDGSharedMemoryPass.cpp:233-260`). fds in our
   world need handle-style translation across cages exactly as CALI needs
   SCM_RIGHTS passing.
7. **Match CALI's residue philosophy but invert the fallback.** CALI's "when
   unsure, share more" is unsound for *us* (sharing isn't possible) — our safe
   fallback when size/direction is unknown must be **INOUT + conservative
   (max) size + a warning**, or `FORCE_LOCAL`, never silent under-copy. Keep
   CALI's "emit a warning, don't fail the build" ergonomics
   (`warnForPossiblyDangerousActions`).
8. **`function_behavior` is a tiny, high-leverage annotation surface.** A handful
   of "X behaves like malloc/free" hints unlocked ImageMagick/socat for CALL with
   no source changes. We should expose the same minimal knob for custom
   allocators and opaque-handle classes rather than demanding full IDL.
