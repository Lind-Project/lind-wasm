# Lind Library-Function Argument Marshalling Plan

*A living plan: for each category of library-function argument, the concrete decision for how
Lind marshals it. Derived from the survey (`report.md`), the KSplit/LXDs analysis
(`ksplit-analysis.md`, `ksplit-lineage.md`, `idl-examples.md`) and `new-design.md` (library-level
3i). Update this as `lind_abi_spec` and the portal/transfer mechanisms evolve.*

---

## How to read this

**Status legend**
- ✅ **CONCRETE** — works today (flat-marshalling prototype) or is a trivial `lind_abi_spec` extension.
- 🔨 **TODO** — design is clear and feasible in Lind; needs implementation.
- ⚠️ **CONSTRAINED** — solvable for some placement models, not all (the plan says which).
- ⛔ **IMPOSSIBLE / DON'T** — cannot be done soundly in a placement model; the plan says why and what to do instead.

**The three placement models** (a handler chooses one; the descriptor is the same — see `report.md` §6.1):
- **IC** = inter-cage (same machine, separate Wasm linear memories; trusted `copy_data_between_cages`).
- **IP** = inter-process (same machine, Unix socket; serialize).
- **IM** = inter-machine (network, TCP; serialize; addresses fully meaningless on the far side).

**The one Wasm fact that drives everything:** a "pointer" argument is a **32-bit offset into the
caller cage's linear memory**. It is only meaningful relative to that cage's `mem_base`. So every
pointer is either (a) **copied** (read the bytes at `mem_base+offset`, reproduce them on the far
side), or (b) **translated as an opaque token** (it names something the far side owns). Picking
(a) vs (b) per argument is the entire problem.

**Trusted vs untrusted (from `new-design.md` §8 / `report.md` §6.2):** the trusted core provides only
`safe_copy(region, dir)` + bounds/cage validation + `translate_handle(class, token)`. Routing,
transport, and the projection-walking glue are untrusted (hand-written now, generated later).

---

## Category status at a glance

| # | Category | IC | IP | IM | Status |
|---|----------|:--:|:--:|:--:|--------|
| 1 | Scalar (int/float by value) | ✅ | ✅ | ✅ | CONCRETE |
| 2 | Fixed-size flat pointee | ✅ | ✅ | ✅ | CONCRETE |
| 3 | C-string (`char*`, NUL-term) | ✅ | ✅ | ✅ | CONCRETE |
| 4 | Counted buffer `(ptr,len)` | ✅ | ✅ | ✅ | CONCRETE |
| 5 | Output / in-out buffer | ✅ | ✅ | ✅ | CONCRETE |
| 6 | Size-via-pointee (`*lenarg`) | ✅ | ✅ | ✅ | CONCRETE |
| 7 | Sentinel-terminated array | 🔨 | 🔨 | 🔨 | TODO |
| 8 | Nested / transitive struct (`z_stream`) | 🔨 | 🔨 | 🔨 | TODO (flagship) |
| 9 | Tagged / discriminated union | 🔨 | 🔨 | 🔨 | TODO |
| 10 | Opaque handle (`FILE*`, fd, internal state) | 🔨 | 🔨 | 🔨 | TODO (high priority) |
| 11 | Pointer-to-pointer / out-handle (`T**`) | 🔨 | 🔨 | 🔨 | TODO |
| 12 | Callback / function pointer | 🔨 | ⚠️ | ⛔ | CONSTRAINED |
| 13 | Polymorphic `void*` (type unknown) | ⚠️ | ⚠️ | ⚠️ | CONSTRAINED |
| 14 | Pointer **return** value (malloc/strtok) | ⚠️ | ⚠️ | ⛔ | CONSTRAINED / IMPOSSIBLE |
| 15 | Wasm-memory bounds & special regions | ✅ | ✅ | ✅ | CONCRETE (safety) |
| — | errno / side effects (cross-cutting) | ✅ | ✅ | ✅ | CONCRETE |
| — | Stateful-library affinity (cross-cutting) | ✅ | 🔨 | 🔨 | TODO |
| — | Object lifetime / free (cross-cutting) | 🔨 | 🔨 | 🔨 | TODO |

---

## Per-category plans

### 1. Scalar (int / float by value) — ✅ CONCRETE
`i32/i64/f32/f64`. No address, no translation. **Plan:** copy the raw value into the request;
`lind_abi_spec` already covers it (`LIND_ABI_I32/I64/F32/F64`). Same for all placements. Done.

### 2. Fixed-size flat pointee (`struct stat *`, no internal pointers) — ✅ CONCRETE
**Plan:** `safe_copy` of `sizeof(T)` bytes from `mem_base+offset`, direction per `ptr_direction`.
`lind_abi_spec` `LIND_ABI_PTR` + `LIND_SIZE_CONST`. Works today for structs **without** internal
pointers. (Structs *with* internal pointers → category 8.)

### 3. C-string (NUL-terminated `char*`) — ✅ CONCRETE
**Plan:** scan from `mem_base+offset` to `\0`, **capped** (prototype cap = 4096) to bound a hostile
or corrupt input; copy `[in]`. `LIND_SIZE_CSTR`. Done. *Note:* keep the cap configurable; an
uncapped scan is a DoS/abort risk.

### 4. Counted buffer `(ptr, len)` — ✅ CONCRETE
The `crc32(crc, buf, len)` case. **Plan:** size from a sibling scalar arg
(`LIND_SIZE_FROM_ARG_VALUE`, `size_arg_index`); `safe_copy` that many bytes, direction per spec.
Done in the prototype.

### 5. Output / in-out buffer — ✅ CONCRETE
**Plan:** `LIND_PTR_OUT` → allocate on the far side, **don't** copy in, copy back on return;
`LIND_PTR_INOUT` → copy both ways. The `compress2` `dest` case. Done (flat case).

### 6. Size-via-pointee (length in `*lenarg`, e.g. `compress2`'s `destLen`) — ✅ CONCRETE
**Plan:** two-pass size resolution — read the `uLong` at `*size_arg` first, then size the buffer.
`LIND_SIZE_FROM_ARG_POINTEE`. Done in the prototype.

### 7. Sentinel-terminated array (`argv`, NULL-terminated pointer tables) — 🔨 TODO
**Plan (feasible now):** add `LIND_SIZE_SENTINEL(value)` — scan elements of a known stride until the
sentinel (usually 0). For arrays of **scalars** this is a simple bounded scan. For arrays of
**pointers** (`char **argv`) it is recursive: each element is itself a category-3/8 pointer needing
translation — so it composes with category 8's walker. Bound the scan length. Moderate effort;
do it together with category 8.

### 8. Nested / transitive struct — the `z_stream` case — 🔨 TODO (flagship work item)
A pointer to a struct whose fields are themselves pointers (input/output buffers, internal state),
possibly recursive/cyclic. **This is the central gap and the main thing to build.**
**Plan (KSplit/LXDs model, see `idl-examples.md`):**
1. Extend `lind_abi_spec` with a **recursive `lind_layout`** (a projection): per-field offset +
   nested `lind_arg_spec` + a `touched` bit (copy only used fields — the projection optimization).
2. Untrusted **generated/hand-written glue walks the layout**; for each pointer field it issues a
   `safe_copy` (scalars) or recurses (nested pointers), maintaining a **visited-set** so cycles
   terminate.
3. **Pointer fixup is placement-specific:**
   - **IC:** allocate a shadow region in the *target cage's* linear memory (via a trusted
     bump/`malloc` helper), copy the pointee there, and rewrite the parent's pointer field to the
     **target-cage offset**.
   - **IP/IM:** serialize the tree (offsets become stream positions, à la XDR); the far side
     rebuilds it in its own memory. No address survives the wire.
4. The internal `state` pointer inside `z_stream` is **not** copied — it is category 10 (handle).
**Why TODO not CONCRETE:** requires the recursive descriptor, the walker, shadow allocation in the
target cage, and pointer fixup — none of which the flat prototype has. Everything needed exists in
Lind (we control the cage memory and have `copy_data_between_cages`); it's build work, not a blocker.
**Interim:** a Sandcrust-style "serialize the whole reachable tree" mode gets correctness first,
optimize with projections later.

### 9. Tagged / discriminated union — 🔨 TODO
**Plan:** `lind_layout` union variant + a **discriminator** (which arg/field selects the active
arm, à la MIDL `switch_is` / KSplit discriminator fn). Marshal only the active arm. Feasible; the
discriminator must be supplied in the spec (or by a small handler-provided function). Lower priority
— rare in the target libraries (zlib/openssl public APIs use few exposed unions). Expect this to be
the residual-warning case, as it is in KSplit.

### 10. Opaque handle (`FILE*`, fd, GPU-style handle, `z_stream`'s internal `state`) — 🔨 TODO (high priority)
A value the library owns; the app must **never** dereference it. **The category whose mishandling
corrupts memory, and the one the current `lind_abi_spec` entirely lacks.**
**Plan (rCUDA/GVirtuS + LXDs `bind` model):**
1. Add `LIND_ABI_HANDLE { handle_class }`.
2. Trusted runtime keeps a **per-boundary shadow handle table**: bidirectional map
   `app-token ↔ real-object` (a real pointer in the library-host cage for IC; a remote id for IP/IM).
3. On the way out: replace the real value with an opaque token; on the way in: `translate_handle`
   back. Never copy the pointee.
4. Because it is a capability, the **table lives in the trusted core** (LXFI lesson; forging a
   handle must be impossible). `alloc`/`bind`/`dealloc` lifecycle (LXDs) manages entries.
Feasible in all placements; this is the single highest-value addition.

### 11. Pointer-to-pointer / out-handle (`T**`, callee allocates) — 🔨 TODO
`getline`, `cudaMalloc`-shaped. **Plan:** add `LIND_SIZE_FROM_ARG_AFTER_CALL` (MIDL
`size_is(,*p)`): callee allocates, true size known only after the call; allocate a shadow / copy
back, and register the returned object in the handle table (category 10). Moderate; build with 10.

### 12. Callback / function pointer — 🔨/⚠️/⛔ CONSTRAINED
The arg is a **Wasm `__indirect_function_table` index valid only in the caller cage**.
- **IC — 🔨 TODO, feasible:** Lind already has cross-cage invocation (3i). Install a **reverse
  portal**: when the library invokes the callback, trap and dispatch back into the *caller* cage's
  table with the original index (carry caller-cage context as a hidden argument, à la LXDs). Build
  work, but the primitive exists.
- **IP — ⚠️ CONSTRAINED:** possible only with a reverse-RPC channel back to the caller process;
  high latency, careful re-entrancy. Implement only if a real use case needs it.
- **IM — ⛔ DON'T:** the callback's *code and closure state live only in the caller's address space*
  and cannot be shipped; synchronous reverse-RPC across a network per callback invocation is
  impractical. **Plan:** the scheduler must **refuse remote routing** for functions with callback
  args (force Local or IC). Mark such functions in the spec.

### 13. Polymorphic `void*` (concrete type known only at the call site) — ⚠️ CONSTRAINED
**Plan:** the spec author (or future inference) resolves the concrete type per function → then it
becomes category 2/8. If unresolved: **treat as an opaque handle (cat 10) and force Local/IC**, and
emit a warning — **never silently deep-copy an unknown `void*`** (KSplit's 7 misclassifications were
exactly this failure). Concrete *as a safe default*; full resolution is the inference TODO.

### 14. Pointer **return** value (`malloc`, `strtok`, `localtime`, `dlsym`) — ⚠️/⛔ CONSTRAINED→IMPOSSIBLE
The function returns a pointer. Three sub-cases:
- **Returns a handle the app only passes back** (e.g. `FILE*`): ✅ via category 10 — register, return
  a token. Works all placements.
- **Returns a pointer to data the app will read** (`localtime` → static struct, known size): ⚠️
  copy-out the pointee into the app cage and return a translated offset — works **if the size is
  known**; otherwise warn/Local.
- **Returns freshly-allocated memory the app uses as its own** (`malloc`, `strdup`): ⛔ **IMPOSSIBLE
  for IP/IM** — a remote allocation cannot become a valid pointer in the app cage's linear memory.
  **Plan:** force such functions **Local** (or IC where the allocation can be made in the app cage
  via a trusted helper). Reason: there is no way to make an off-cage address dereferenceable in-cage
  without re-allocating in-cage and copying, and the size/lifetime is generally unknown.

### 15. Wasm-memory bounds & special regions — ✅ CONCRETE (safety)
The Lind analog of KSplit's "user vs kernel memory." **Plan:** the trusted `safe_copy` **always**
interprets app pointers as `mem_base+offset` and **bounds-checks** `offset+size` against the cage's
linear-memory size before copying (reject otherwise). This is mandatory safety, cheap, and present
in spirit in the prototype. *Future:* a `shmem://` zero-copy fast path (BGI-style per-region access
rights instead of copying) for same-machine large buffers — TODO, optional optimization.

---

## Cross-cutting concerns

### errno / side effects — ✅ CONCRETE
Already shipped in the remote wire protocol (`errno` in every response, written back into the app
cage). Keep this for all placements.

### Stateful-library affinity — ✅ IC / 🔨 IP,IM
Stateful libraries (`strtok`, PRNGs, a live `z_stream`) require related calls to hit the **same
instance**. **Plan:** IC is natural (one library-host cage). IP/IM need **sticky routing**: pin a
session to one endpoint/connection, keyed by the relevant **handle** (cat 10) — the handle that
names the state also pins the instance. TODO in the scheduler.

### Object lifetime / free across the boundary — 🔨 TODO
Who frees a shadow copy / handle, and when. **Plan:** `alloc`/`bind`/`dealloc` qualifiers (LXDs) on
the descriptor + a hybrid static/dynamic tracker (KSplit): the handle table owns shadow objects and
frees them on the matching `dealloc` call (e.g. `deflateEnd`, `fclose`, `free`). Build with cat 10.

---

## Priority / staged roadmap

1. **Lock in CONCRETE (1–6, 15, errno).** Confirm the flat prototype covers them under the new
   `register_lib_handler`/`lind_abi_spec` path; add the mandatory bounds-check (15).
2. **Build the handle table (10) + lifetime (cross-cutting) + affinity.** Highest value, unblocks
   `z_stream`'s `state`, `FILE*`, fds, out-handles (11), and pointer-returns (14).
3. **Build the recursive projection walker (8) + sentinel arrays (7).** The flagship transitive-copy
   work; ship a serialize-everything interim mode first, then add projections.
4. **Callbacks (12) for IC** via reverse portal; mark callback-bearing functions non-remotable.
5. **Unions (9)** and **`void*` resolution (13)** as the residual / warning cases.
6. **Later:** KSplit-style inference to auto-emit the descriptor (see `ksplit-lineage.md` staging),
   and a `shmem://` zero-copy path (15).

**Guiding rule (the safe default):** when a pointer's category is uncertain, treat it as an
**opaque handle + force Local/IC and warn** — never silently deep-copy. Misclassifying a buffer is a
memory-corruption bug; misclassifying toward "handle + Local" is merely a missed optimization.
