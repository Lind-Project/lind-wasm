# KSplit idlc — IDL Grammar & Codegen Analysis

Read-only study of the KSplit IDL compiler at `/home/lind/inference-refs/idlc`, the
analogous code-gen stage to our auto-marshalling emitter. All file:line refs are real and
spot-checkable. Our target spec types live in
`/home/lind/lind-wasm/tests/grate-tests/lib-interpose/lind_marshal.h` and a hand-written
example of the C we must emit is
`/home/lind/lind-wasm/tests/grate-tests/lib-interpose/auto-nested/auto-nested_grate.c`.

> TL;DR of the architectural difference: idlc is a **two-domain RPC stub generator**. It
> emits *static, per-projection, per-direction* C "visitor" functions that serialize a struct
> field-by-field into a `fipc_message` wire buffer (`glue_pack`/`glue_unpack`), to be
> transmitted between a kernel LCD and a driver. We instead emit/interpret a *data-driven*
> `lind_marshal_spec` table and a single generic runtime
> (`lind_marshal_dispatch`) that copies between two wasm cages' linear memories with
> `copy_data_between_cages`. idlc's "where is the size / direction / projection" *decisions*
> map almost 1:1 onto our spec fields; its *mechanism* (FIPC registers, shadow hashmaps,
> trampolines) is kernel-specific and irrelevant to us.

---

## 1. The IDL surface grammar (from `source/parser/idl.peg`)

The grammar is a PEG consumed by the bundled **vembyr** PEG→C++ generator. `CMakeLists.txt:14-16`
runs `python2 .../vembyr-1.1/peg.py --cpp idl.peg` at build time to produce `parser.cpp`/`parser.h`.
Each rule has an inline `{{ ... }}` C++ action that builds AST nodes (`ast/ast.h`). So **the
`.peg` is literally the schema of the inference output** — whatever our inference stage produces
must serialize to these constructs.

Top-level shape (`idl.peg:204-300`): a file is either a `driver { import ...; }` block or a list
of `module name { ... }` blocks. A module holds **projections**, **rpc** defs, **rpc_ptr** defs,
**rpc_export** defs, **globals**, and **locks**.

### 1.1 RPC = one marshallable function (≈ our `lind_marshal_spec` per intercepted symbol)

`idl.peg:647-675`:
```
rpc        <ret_type> name ( <arg-list> ) { <projection-defs / nested rpc_ptr defs> }
rpc_ptr    <ret_type> name ( <arg-list> ) { ... }   # function-pointer / callback type
rpc_export <ret_type> name ( <arg-list> ) { ... }   # symbol the driver exports back
```
- `rpc` (`rpc_def_kind::direct`) = callee lives kernel-side; driver calls it. This is our normal
  "intercept a libc/zlib symbol" case.
- `rpc_ptr` (`indirect`) = a callback whose body lives driver-side; reached via a trampoline. Maps
  to our "function-pointer argument that must be re-exposed to the other cage" problem.
- `rpc_export` = inverse direction. No analogue we need yet.

The `{ ... }` body lists, per argument, a **projection** = the *subset of struct fields actually
touched* — KSplit's field-level reachability result. This is exactly our `lind_field.touched`
projection concept (`lind_marshal.h:112-116`), but expressed positively (you only list touched
fields) instead of with a per-field flag.

### 1.2 `var_decl` and `type_spec` — how a parameter's type is written

`var_decl = type_spec name` (`idl.peg:384`). A `type_spec` (`idl.peg:631-645`) is:
`[const] [volatile] <stem> <indirection>* [val_attrs]`, where each `*` (indirection) may carry its
own `[ptr_attrs]` **before** the star (`idl.peg:627`: `attrs:indirection_attrs tok_star`).

- **stem** (`idl.peg:609-623`): a primitive (`int`,`u32`,`char`,…; `idl.peg:523-567`),
  `string` (sugar for `array<char, null>`, `idl.peg:583` + `analysis.cpp:31-35`), `array<T,size>`,
  `projection NAME`, `rpc_ptr NAME`, `casted<decl,real>`, or `void`.
- **value attrs** `[in]`/`[out]`/`[unused]` (`idl.peg:405-407`) — direction of the *pointee value*.
- **pointer attrs** (`ptr_attr`, `idl.peg:409-488`) — direction/ownership of the *pointer itself*.

### 1.3 Directions — `in` / `out` / `in out` (≈ our `lind_ptr_direction`)

Direction is an attribute list. `[in]`, `[out]`, or `[in, out]` (`idl.peg:494-518` parses the
comma list and ORs the bits). Bits defined in `tag_types.h:21-22` (`in = 1<<6`, `out = 1<<7`) with
`in_out = out|in` (`:44`). Direction can sit on the value (`u32 [out] nsec;` —
`examples/mei-me`/`ixgbe.idl:17`) or on a pointer indirection (`* [out]`). Our exact mapping:

| idlc | ours (`lind_ptr_direction`) |
|---|---|
| `[in]` | `LIND_PTR_IN` |
| `[out]` | `LIND_PTR_OUT` |
| `[in, out]` | `LIND_PTR_INOUT` |

If omitted, defaults propagate: arguments default to `in`, return values to `out`
(`analysis.cpp:383,391`), pushed down the pgraph by `annotation_walk` (`analysis.cpp:218-247`).

### 1.4 Pointers, sizes, and `array<T, size>` (≈ our `size_kind`)

There is **no `size`/`len` keyword**; a pointer-with-count is written as `array<T, size>`
(`idl.peg:570-580`). The size slot is one of three (`array_size`, `idl.peg:574-580` →
`ast.h:58` `using array_size = variant<unsigned, tok_kw_null, ident>`):
- `array<T, N>` — numeric literal ⇒ `static_array` (`pgraph.h:100`). ≈ `LIND_SIZE_CONST`.
- `array<T, null>` — null-terminated ⇒ `null_terminated_array` (`pgraph.h:77`). `string` is sugar
  for this over `char`. ≈ `LIND_SIZE_CSTR`.
- `array<T, {{ verbatim C expr }}>` — a `tok_verbatim_expr` (`idl.peg:194`) ⇒ `dyn_array` carrying
  the literal C string (`pgraph.h:90`, `dyn_array.size_expr`). The expr can reference sibling
  fields, e.g. `array<char, {{ptr->msg_namelen}}>` (`examples/mwe-msg_hdr.idl:10`). ≈
  `LIND_SIZE_FROM_ARG` (sibling field) — but idlc encodes it as a **textual C expression**, not a
  field index. There is no first-class `FROM_ARG_POINTEE`; you would write
  `array<T, {{*lenp}}>` as a verbatim expr.

A plain `T*` with no `array<>` is just a pointer to one `T` (size = `sizeof(*referent)` via
`get_size_expr`, `walks.h:351-363`). ≈ `LIND_SIZE_CONST` with `const_size = sizeof`.

### 1.5 Projections — field subsets (≈ our nested `lind_layout` + per-field `touched`)

`idl.peg:311-318`:
```
projection < struct net_device > dev {
    unsigned long long [out] features;
    const projection _global_netdev_ops *netdev_ops;   # nested projection ptr field
    unsigned int  flags;
}
```
`projection <struct REALNAME> ALIAS { fields }`. `REALNAME` is the actual C struct
(`proj_def.type`, emitted as `struct REALNAME` in generated code); `ALIAS` is how arguments refer
to it. **Only listed fields are marshalled** — untouched fields are silently skipped. This is
KSplit's whole-program field-reachability output and is our `touched=1` set
(`lind_marshal.h:112-116`). Bitfields are supported (`field : width;`, `idl.peg:374-376`) and get
special copy-in/out handling (`generation.cpp:265-289`, `fixup_bitfields` at `:779-801`).

A field that is itself a `projection NAME *` recurses — that is idlc's nested-struct mechanism,
matching our `lind_field.spec` recursion. Unions exist syntactically (`projection <union ...>`,
`idl.peg:320-327`) but are stubbed (`analysis.cpp:111-116` "not yet implemented") — comparable to
our declared-but-thin `LIND_LO_UNION`.

### 1.6 Opaque handles / resources / ownership (≈ our `LIND_ARG_HANDLE`)

idlc has **no opaque-token handle table**. The nearest constructs are *ownership/binding*
annotations on pointers (`ptr_attr`, `idl.peg:409-488`):
- `bind(caller)` / `bind(callee)` — "the shadow copy already exists statically on this side; look
  it up in the shadow hashmap rather than deep-copying" (`tag_types.h:19-20`, semantics in
  `walks.h:124-138`, `glue_pack_shadow`/`glue_unpack_shadow`). This is the **closest analogue to our
  handle**: a pointer that is *translated through a side-local table, never deep-copied*. Our
  `lind_register_handle`/`lind_translate_handle` (`lind_marshal.h:210-242`) is the runtime form of
  exactly this idea.
- `alloc(caller|callee)` / `alloc_once` / `dealloc(...)` — lifetime: allocate/free the shadow on a
  given side (`walks.h:539-576`, `605-627`).
- `bind_memberof<struct T, field>(...)` — pointer into the middle of a tracked object; recovered via
  `container_of` (`walks.h:155-158`).
- `shared<global>` — pointer into a statically shared region; marshalled as an offset
  (`walks.h:179-182`).
- `ioremap(caller)` — MMIO BAR remap; pure kernel-ism (`generation.cpp:325-361`).
- `casted<void*, realtype *>` (`idl.peg:593-595`) — a `void*` that is "really" another type; lets
  inference attach a real layout to an opaque field (`examples/mwe-msg_hdr.idl:10`).

So idlc spreads our single `LIND_ARG_HANDLE` concept across `bind`/`alloc`/`shared`; the unifying
idea ("translate a pointer through a side-local table, do not copy the pointee") is `bind`.

### 1.7 Return values (≈ our `lind_return_spec`)

The return type is just the `type_spec` before the rpc name (`idl.peg:647`). It is lowered into a
`ret_pgraph` and marshalled with `out` as default direction (`analysis.cpp:383`,
`generation.cpp:227-244` allocates `ret`/`ret_ptr`). Pointer returns can carry the same ownership
annotations — e.g. `rpc projection ret_net_device [alloc(caller)]* alloc_netdev_mqs(...)`
(`examples/nullnet.idl:39`) returns a freshly-allocated, deep-copied projection (≈ our
`LIND_RET_HANDLE`/new object). `[ioremap(caller)]` returns translate a BAR
(`generation.cpp:324-361`). idlc has **no** equivalent of our `LIND_RET_PTR_ALIAS_ARG` /
`LIND_RET_PTR_INTO_ARG` (return-the-original-arg-pointer, or a pointer *into* an arg's buffer) —
those are wasm-cage-pointer-identity concerns that don't arise in a copy-everything RPC model.

---

## 2. Compiler pipeline (parse → AST → passes → emit)

Driver is `source/main.cpp:45-97`:

1. **Parse** (`main.cpp:53`): `parser::parse_file` runs the vembyr-generated `parser.cpp` over the
   `.peg`; `{{}}` actions build the AST (`ast/ast.h`). AST node types: `rpc_def`
   (`ast.h:316-373`), `proj_def` (`ast.h:263-289`), `var_decl`, `type_spec`/`type_stem`/`indirection`,
   `annotation` (`tag_types.h:60-82`).
2. **Name binding** (`main.cpp:58`, `frontend/name_binding.cpp`): resolves `projection NAME` /
   `rpc_ptr NAME` references to their defs (fills `type_proj::definition`, `type_rpc::definition`).
3. **Inject global rpcs** (`main.cpp:63`, `frontend/injection.h`): synthesizes lock acquire/release
   RPCs from `spinlock`/`atomic_lock` decls; re-binds names (`main.cpp:67`).
4. **string→const pass** (`main.cpp:72-74`, `string_const_walk`): marks `string` types const.
5. **AST → pgraph lowering** = the real "passes" (`main.cpp:76`,
   `frontend/analysis.cpp:466 generate_all_pgraphs`):
   - `create_pgraphs_from_types` (`analysis.cpp:160-172`) turns each `type_spec` into a `value`
     tree (`pgraph.h:46-61`) via `generate_value` (`analysis.cpp:137-158`): each `*` becomes a
     `pointer` node wrapping a `value`; `array<>` becomes `static/dyn/null_terminated_array`;
     `projection NAME` is **deferred** as a raw `proj_def*` (`analysis.cpp:47-49`).
   - `annotate_pgraphs` (`analysis.cpp:381-415`): default-direction propagation
     (`annotation_walk`, `:209-272`) and **lazy projection instantiation** — a projection is
     cloned once per direction it's reached with (`__in`/`__out`/`__io` instances,
     `instantiate_projection` `:304-343`, cached in `proj_def::in_proj/out_proj/in_out_proj`,
     `ast.h:270-272`). This direction-specialization is why idlc emits *four* visitor functions per
     projection.
   - `const_walk` (`:345-379`) propagates constness to array elements.
   - `rpc_context_walk` (`:417-439`) links each projection to its parent rpc (for the `call_ctx`).
6. **Emit** (`main.cpp:96`, `backend/generation.cpp:947 generate`):
   - `get_projections` (`:930`) collects all reachable projections.
   - `populate_c_type_specifiers` (`backend/c_specifiers.cpp`) renders each pgraph node's C type
     string into `value::c_specifier`.
   - `create_function_strings` (`:727`) precomputes `ret_string`/`args_string`/`params_string`.
   - Then writes 5 files: `common.h` (`:104`), `common.c` (`:882`), `client.c` (`:631`),
     `server.c` (`:668`), `trampolines.lds.S` (`:709`).

The **per-construct emitters** are the `marshaling_walk` / `unmarshaling_walk` pgraph visitors in
`backend/walks.h` — one `visit_*` method per pgraph node kind. These are the direct analogue of the
`_lind_pre_ptr` / `_lind_post_struct` / `_lind_compute_size` logic in our `lind_marshal.h`, except
ours is **one generic interpreter over a data table** and idlc's is **inlined C generated per type**.

---

## 3. Before → After: IDL → generated C

The four visitors per projection are named `caller_marshal_*`, `callee_unmarshal_*`,
`callee_marshal_*`, `caller_unmarshal_*` (`pgraph.h:173-179`). "caller" = driver side (`client.c`),
"callee" = kernel side (`server.c`); marshal = serialize into the FIPC message, unmarshal = read out.
`__pos` walks the register buffer; `glue_pack`/`glue_unpack` are the wire primitives
(`helpers.cpp:9-11`). Snippets below are exact templates from the emitters.

### 3a. Scalar (`u32 [out] nsec;`)
Emitter: `marshaling_walk::visit_primitive` (`walks.h:208-212`) and
`unmarshaling_walk::visit_primitive` (`:395-399`).
```c
// marshal side:   glue_pack(__pos, __msg, __ext, *nsec_ptr);
// unmarshal side: *nsec_ptr = glue_unpack(__pos, __msg, __ext, u32);
```
≈ our `LIND_ARG_SCALAR`: `handler_args[i] = raw_args[i]` (`lind_marshal.h:590-592`). idlc packs the
value into the wire; we pass it by value with no copy. Direction gating
(`should_marshal`/`should_unmarshal`, `walks.h:72-100`) decides which side packs vs unpacks — our
`ptr_direction` decides copy-in vs copy-out.

### 3b. Sized buffer / ptr+len  (`u8 [in] *buf, u64 count` → `array<u8,{{count}}>`)
Pointer emitter `marshaling_walk::visit_pointer` (`walks.h:148-206`) packs the pointer
(`glue_pack(... __adjusted)`, `:189`), then `visit_dyn_array` (`:274-293`) packs the length and
loops the elements:
```c
size_t i, len = (count);
u8 const* array = buf_ptr;
glue_pack(__pos, __msg, __ext, len);
for (i = 0; i < len; ++i) { u8 const* element = &array[i]; glue_pack(... *element); }
```
Unmarshal side (`visit_dyn_array`, `:483-498`) allocates the shadow and loops `glue_unpack`. ≈ our
`LIND_ARG_PTR` + `LIND_SIZE_FROM_ARG` (`_lind_compute_size` reads the sibling/arg, then
`copy_data_between_cages` of `size` bytes, `lind_marshal.h:323-328, 388-393`). **Key contrast:** idlc
emits the size as a *verbatim C expression* (`{{count}}`) inlined into the generated loop; we store a
*field/arg index* (`size_arg_index`) the runtime dereferences. `null`-array ⇒ our `CSTR`
(`visit_null_terminated_array`, `:228-255` scans for the sentinel, packs `len+1`; our
`_lind_measure_cstr`, `lind_marshal.h:248-267`).

### 3c. Nested projection / struct
```
rpc void register_netdevice( projection dev *dev ) {
    projection < struct net_device > dev {
        unsigned long long [out] features;
        const projection _global_netdev_ops *netdev_ops;
    }
}
```
`visit_pointer`→`visit_projection` (`walks.h:220-226`) emits a *call to the per-projection visitor*:
```c
caller_marshal_dev__in(__pos, __msg, __ext, __visited, ctx, dev_ptr);
```
and `generate_caller_marshal_visitor` (`generation.cpp:754-777`) emits that function: it takes
`struct net_device const* ptr`, makes a root pointer per touched field
(`generate_root_ptrs<marshaling,caller>`, `:253-294` → `features_ptr = &ptr->features;`), and recurses
field-by-field. The nested `netdev_ops` projection ptr triggers another visitor call. `__visited`
(`helpers.cpp:146-190`) is a cycle guard (≈ our shadow-tracking; we don't yet de-dup cycles). ≈ our
`lind_layout` walk in `_lind_pre_ptr` (`lind_marshal.h:396-474`): iterate `fields`, skip
`!touched`, recurse on PTR fields, blit scalars, translate HANDLE fields. idlc generates one
specialized function per (projection × direction); we run one generic loop over `lind_field[]`.

### 3d. Handle / bound resource (`projection sock [alloc(callee)] *sock`, or any `bind(callee)` ptr)
`alloc(callee)`: `marshal_pointer_value` (`walks.h:605-617`) emits
```c
size_t __size = sizeof(struct socket);
*sock_ptr = glue_unpack_new_shadow(__pos, __msg, __ext, struct socket*, (__size), (DEFAULT_GFP_FLAGS));
```
`bind(callee)`: `glue_pack_shadow`/`glue_unpack_shadow` (`walks.h:177-178`, `:618-621`) translate the
pointer through the side-local shadow hashmap (`glue_user_map_to_shadow`, `helpers.cpp:131-138`)
**without copying the pointee** — this is the conceptual twin of our handle table. ≈ our
`LIND_ARG_HANDLE`: `handler_args[i] = lind_translate_handle(class, raw_args[i])`
(`lind_marshal.h:594-595`) and in-struct field translation (`:414-420`). Difference: idlc's shadow
map is keyed by *real pointer identity* and shared structurally; our handle table mints opaque
*tokens* and never exposes real pointers cross-cage (a security property idlc doesn't need
between two trusted kernel domains).

### 3e. Return translation (`rpc projection ret [alloc(caller)]* foo(...)`)
Return is marshalled like an `[out]` arg through `ret_ptr` (`generation.cpp:372-380, 519-527`):
callee packs `ret` into the message, caller unpacks into `ret` and `return ret;`
(`:389`). A pointer return with `alloc(caller)` deep-copies a new object on the caller side. ≈ our
`LIND_RET_SCALAR` (return handler value) / `LIND_RET_HANDLE` (register & return a token,
`lind_marshal.h:682-683`). idlc has no `RET_PTR_ALIAS_ARG`/`RET_PTR_INTO_ARG` equivalent — those are
our pointer-identity translations (`:657-680`) needed because a wasm cage pointer must be rewritten
to the *source cage's* address space, a problem absent in idlc's copy-by-wire model.

---

## 4. Mapping table: our `lind_marshal_spec` → idlc IDL construct → generated-C pattern

| our concept (`lind_marshal.h`) | idlc IDL construct (`idl.peg`) | idlc pgraph node | generated-C pattern |
|---|---|---|---|
| `LIND_ARG_SCALAR` | bare primitive arg (`int x`) | `primitive` | `glue_pack(*x_ptr)` / `*x_ptr = glue_unpack(...,int)` (`walks.h:208,395`) |
| `LIND_ARG_PTR` | `T* x` / `array<T,..> x` | `pointer`(+`*_array`) | `visit_pointer` packs ptr, recurses pointee (`walks.h:148-206`) |
| `LIND_ARG_HANDLE` | `[bind(caller/callee)] *` | `pointer`+bind annot | `glue_pack_shadow`/`glue_unpack_shadow` (`walks.h:177,618`) |
| `ptr_direction IN/OUT/INOUT` | `[in]`/`[out]`/`[in,out]` | `value.value_annots` | `should_marshal`/`should_unmarshal` gate (`walks.h:72-100`) |
| `LIND_SIZE_CONST` | `array<T,N>` or bare `T*` | `static_array` / `sizeof` | numeric `len` / `sizeof(*ref)` (`walks.h:257-272,351-363`) |
| `LIND_SIZE_FROM_ARG` | `array<T,{{sibling}}>` | `dyn_array.size_expr` | `size_t len=(expr); for(...)` (`walks.h:274-293`) |
| `LIND_SIZE_FROM_ARG_POINTEE` | `array<T,{{*lenp}}>` (verbatim) | `dyn_array` | same loop, expr deref'd inline |
| `LIND_SIZE_CSTR` | `string` / `array<T,null>` | `null_terminated_array` | scan to sentinel, pack `len+1` (`walks.h:228-255`) |
| nested layout + per-field `touched` | `projection<struct R> A { fields }` | `projection` | one `*_marshal_*` visitor fn, field roots (`generation.cpp:754-880`) |
| `lind_field.offset/spec` recursion | nested `projection NAME *field` | `projection_field` | nested visitor call (`walks.h:220-226`) |
| `LIND_RET_SCALAR` | `rpc int name(...)` | `ret_pgraph` primitive | pack/unpack `ret` (`generation.cpp:519-527`) |
| `LIND_RET_HANDLE` / new obj | `rpc projection r [alloc(caller)]* name` | `ret_pgraph` ptr+alloc | `glue_unpack_new_shadow` for ret |
| `LIND_RET_PTR_ALIAS_ARG`/`INTO_ARG` | *(none)* | — | n/a (copy-by-wire, no ptr identity) |
| `handle_class` | *(implicit per shadow type)* | — | shadow keyed by real ptr type |
| `flags` / `alloc`/`dealloc` lifetime | `alloc(...)`,`dealloc(...)`,`alloc_once` | annotation bits | `glue_unpack_new/bind_or_new`, `glue_remove_shadow` |

---

## 5. Lessons for OUR code generator

**Imitate:**
1. **Two-stage lowering, not direct AST→C.** idlc never emits C from the surface syntax; it lowers
   to a small, regular **pgraph** of ~8 node kinds (`pointer`, `*_array`, `projection`,
   `primitive`, `rpc_ptr`, `casted`, `none`) and the emitter is a clean visitor over *that*
   (`analysis.cpp` builds it, `walks.h` walks it). Our emitter should target an equivalent
   normalized IR; the surface inference output should desugar (`string`→cstr-array, `T*`→ptr-of-1)
   before codegen. This keeps the emitter tiny and uniform.
2. **Direction as the single gate.** `should_walk/should_marshal/should_unmarshal`
   (`walks.h:72-117`) derive *every* copy decision from one `in`/`out` annotation plus
   "is-nonterminal". We already encode this as `ptr_direction`; mirror the "always recurse
   non-terminals even if not directly copied, to reach nested `[out]` fields" rule
   (`pgraph.h:64-75`) — it's subtle and easy to get wrong.
3. **Projection = positive touched-field list.** Inference should emit only reached fields with
   their real struct name + offset; everything else is skipped. Our `touched` flag captures this;
   consider emitting the *positive* list to shrink specs.
4. **Per-direction specialization is optional but clean.** idlc clones a projection per direction it
   is reached with (`__in/__out/__io`, `analysis.cpp:304-343`). Because our runtime is a generic
   interpreter, we get this for free at runtime and need not specialize — a genuine simplification
   for us.
5. **Cycle guard.** `__visited` / `glue_should_visit` (`helpers.cpp:179-190`, used in
   `walks.h:197`) prevents infinite recursion on cyclic structs. Our `_lind_pre_ptr` currently has
   **no cycle detection** (`lind_marshal.h:396-474`) — a real gap if we ever marshal self-referential
   structs.
6. **Verbatim size expressions are powerful** (`array<T,{{ptr->len}}>`). Even if we stay
   index-based (`size_arg_index`), keeping an escape hatch for an inferred C expression would handle
   the cases our four `size_kind`s can't.

**Ignore (kernel-specific, irrelevant to wasm cages):**
- **FIPC wire protocol** (`glue_pack`/`glue_unpack` into `fipc_message`/`ext_registers`,
  `helpers.cpp:9-102`). We don't serialize to a register buffer; we `copy_data_between_cages`
  directly between two linear memories. Our model is *shared-nothing copy*, theirs is
  *register-passing*.
- **Trampolines & linker scripts** (`LCD_TRAMPOLINE_*`, `trampolines.lds.S`,
  `generation.cpp:552-572,709-724`). For our `rpc_ptr`/callback case we have grate function-pointer
  re-export (`pass_fptr_to_wt` in the example grate); no executable-section trampolines needed.
- **`ioremap`/MMIO BAR remap, `shared<global>`, percpu** (`generation.cpp:324-361`,
  `walks.h:179-182`) — device/kernel memory concepts with no wasm analogue.
- **Shadow-hashmap binding keyed by real pointer identity** (`glue_user_map_to_shadow`). We
  deliberately use opaque tokens (no real pointers cross the cage boundary) for isolation; keep our
  token table, not their identity map.
- **client.c/server.c split + `try_dispatch` RPC-ID switch** (`generation.cpp:584-707`). Our
  dispatch is `register_lib_handler` + the generic `lind_marshal_dispatch`; we generate *one*
  handler wrapper (`LIND_DEFINE_MARSHAL_HANDLER`, `lind_marshal.h:703-722`) plus a data spec, not
  two mirrored translation units.
- **Bitfield shuffling** (`generation.cpp:265-289`, `fixup_bitfields`) — only needed because you
  can't take `&` of a C bitfield; relevant only if we ever marshal bitfields.

**Net:** idlc validates our overall plan — its IDL fields (`in`/`out`, `array<,>`, `projection`,
`bind`/`alloc`, return annotations) line up almost exactly with our `lind_marshal_spec`. The biggest
design divergence is *static-C-per-type* (idlc) vs *generic-interpreter-over-data* (us); ours is
simpler to emit (one table, no four-visitor explosion) at the cost of a runtime interpreter. Borrow
their pgraph IR shape, their direction-gating rule, and their cycle guard; drop everything FIPC,
trampoline, and MMIO.

---

### Appendix: `linecount`, `setup`, and the `py` files
- `setup` (`/setup`) — one-shot bootstrap shell script: `git submodule update` + vcpkg bootstrap +
  `vcpkg install ms-gsl abseil` (the only two real deps). Not part of codegen.
- `linecount` (`/linecount`) — trivial dev helper: `cloc source/` excluding the vendored parser
  generator and the generated `parser.cpp/.h`. Just SLOC accounting.
- `source/parser/vembyr-1.1/*.py` — **vembyr**, a third-party PEG parser-generator (Python2). Only
  `peg.py` is used, at build time, to compile `idl.peg` → `parser.cpp`/`parser.h` (`CMakeLists.txt:14-16`).
  The other generators (`cpp_*`, `python_generator.py`, `ruby_generator.py`, `lua_generator.py`,
  `regex.py`, `peg_peg.py`, `test_peg.py`) are vembyr's own multi-language backends and self-tests —
  **not** part of idlc proper. Takeaway: idlc treats grammar-as-data and generates its parser; we
  could similarly treat our marshalling-spec schema as the single source of truth.
